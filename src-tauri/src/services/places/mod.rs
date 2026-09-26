//! Place resolution: the AI names a place, a map provider says where it really is.
//! Coordinates only ever come from the provider — never from the LLM.

pub mod scoring;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::models::PlaceCandidate;
use crate::services::ai::types::ExtractedPlace;
pub use scoring::{Decision, PlaceResolutionResult, ResolutionContext};

#[derive(Debug, thiserror::Error)]
pub enum PlaceError {
    #[error("place search is temporarily unavailable: {0}")]
    Unavailable(String),
    #[error("place search is throttled")]
    Throttled,
}

/// A natural-language place search backend (Apple MapKit via the native bridge by default).
#[async_trait]
pub trait PlaceSearchProvider: Send + Sync {
    /// `near` ("Kyoto, Japan" or "Japan") centres the search there instead of on the user's location.
    async fn search(&self, query: &str, near: Option<&str>) -> Result<Vec<PlaceCandidate>, PlaceError>;
}

/// `PlaceService`: resolution with caching, throttling and multi-query fallbacks.
pub struct PlaceService {
    provider: Arc<dyn PlaceSearchProvider>,
    cache: Mutex<HashMap<String, Vec<PlaceCandidate>>>,
    last_request: Mutex<Instant>,
    min_interval: Duration,
}

impl PlaceService {
    pub fn new(provider: Arc<dyn PlaceSearchProvider>) -> Self {
        Self {
            provider,
            cache: Mutex::new(HashMap::new()),
            last_request: Mutex::new(Instant::now() - Duration::from_secs(5)),
            // Apple Maps allows ~50 searches a minute (a "near" hint can add one more); stay below it.
            min_interval: Duration::from_millis(1300),
        }
    }

    /// Overrides the spacing between provider requests (tests use zero).
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    /// Cached, throttled raw search (also used for manual searches from the UI).
    pub async fn search(&self, query: &str) -> Result<Vec<PlaceCandidate>, PlaceError> {
        self.search_near(query, None).await
    }

    pub async fn search_near(&self, query: &str, near: Option<&str>) -> Result<Vec<PlaceCandidate>, PlaceError> {
        let key = format!("{}|{}", crate::text::normalize(query), near.map(crate::text::normalize).unwrap_or_default());
        if let Some(hit) = self.cache.lock().await.get(&key) {
            return Ok(hit.clone());
        }
        let mut attempts = 0;
        let results = loop {
            {
                let mut last = self.last_request.lock().await;
                let wait = self.min_interval.saturating_sub(last.elapsed());
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
                *last = Instant::now();
            }
            match self.provider.search(query, near).await {
                Err(PlaceError::Throttled) if attempts < 2 => {
                    attempts += 1;
                    // Back off; the whole queue shares this limiter, so everyone waits together.
                    let mut last = self.last_request.lock().await;
                    tokio::time::sleep(Duration::from_secs(45 * attempts)).await;
                    *last = Instant::now();
                }
                other => break other?,
            }
        };
        // Show countries in English even when Maps returns a localised name.
        let results: Vec<PlaceCandidate> = results.into_iter().map(|mut c| {
            if let Some(name) = c.country_code.as_deref().and_then(crate::text::country_name) {
                c.country = Some(name.to_string());
            }
            c
        }).collect();
        self.cache.lock().await.insert(key, results.clone());
        Ok(results)
    }

    /// Tries progressively looser queries and returns the best decision.
    pub async fn resolve(&self, place: &ExtractedPlace, ctx: &ResolutionContext, config: &AppConfig) -> Result<PlaceResolutionResult, PlaceError> {
        let mut queries = vec![place.map_query()];
        if let Some(country) = place.country.as_deref().filter(|c| !c.trim().is_empty()) {
            queries.push(format!("{}, {}", place.display_name, country));
        }
        queries.push(place.display_name.clone());
        // Local-language names (e.g. 望仙谷) often work better in that country's map data.
        for alt in place.alternative_names.clone().unwrap_or_default().into_iter().filter(|a| !a.trim().is_empty()).take(2) {
            queries.push(alt);
        }
        queries.dedup();

        // Centre the search on the place's own city/country (not the user's location).
        let area = place.city.as_deref().or(place.region.as_deref());
        let near = match (area.map(str::trim).filter(|c| !c.is_empty()), place.country.as_deref().map(str::trim).filter(|c| !c.is_empty())) {
            (Some(city), Some(country)) => Some(format!("{city}, {country}")),
            (Some(city), None) => Some(city.to_string()),
            (None, Some(country)) => Some(country.to_string()),
            _ => None,
        };
        let mut fallback: Option<PlaceResolutionResult> = None;
        for (index, query) in queries.into_iter().enumerate() {
            // A usable candidate after two query variants is enough: stop early to save Maps calls.
            if index >= 2 && fallback.as_ref().is_some_and(|f| f.decision == Decision::Review) {
                break;
            }
            let results = self.search_near(&query, near.as_deref()).await?;
            // A country or continent is never a pin.
            let results: Vec<PlaceCandidate> = results.into_iter().filter(|c| !crate::text::is_country_or_continent(&c.name)).collect();
            if results.is_empty() {
                continue;
            }
            let ranked = scoring::rank(&results, place, ctx);
            let result = scoring::decide(&ranked, place, config);
            match result.decision {
                Decision::AutoAccept => return Ok(result),
                Decision::Review if fallback.as_ref().is_none_or(|f| f.decision == Decision::Reject) => fallback = Some(result),
                Decision::Reject if fallback.is_none() => fallback = Some(result),
                _ => {}
            }
        }
        Ok(fallback.unwrap_or_else(|| scoring::decide(&[], place, config)))
    }
}
