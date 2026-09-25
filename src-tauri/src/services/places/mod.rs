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
    async fn search(&self, query: &str) -> Result<Vec<PlaceCandidate>, PlaceError>;
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
            // MapKit throttles bursts; stay comfortably below its limit.
            min_interval: Duration::from_millis(400),
        }
    }

    /// Cached, throttled raw search (also used for manual searches from the UI).
    pub async fn search(&self, query: &str) -> Result<Vec<PlaceCandidate>, PlaceError> {
        let key = crate::text::normalize(query);
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
            match self.provider.search(query).await {
                Err(PlaceError::Throttled) if attempts == 0 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                other => break other?,
            }
        };
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
        queries.dedup();

        let mut fallback: Option<PlaceResolutionResult> = None;
        for query in queries {
            let results = self.search(&query).await?;
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
