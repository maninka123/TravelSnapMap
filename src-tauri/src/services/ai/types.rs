//! Structured AI output. Every response is validated locally against these types;
//! anything that fails to parse is rejected, never written to the database.

use serde::{Deserialize, Serialize};

use crate::models::{PlaceCategory, RegionType, TravelFactType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TravelClassification {
    pub is_travel_related: bool,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// One combined request: travel classification + place/fact extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TravelExtraction {
    pub is_travel_related: bool,
    /// Probability (0–1) that the screenshot is travel-related.
    #[serde(default)]
    pub travel_confidence: f64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub places: Option<Vec<ExtractedPlace>>,
}

impl TravelExtraction {
    pub fn all_places(&self) -> Vec<ExtractedPlace> {
        self.places
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.display_name.trim().is_empty())
            .collect()
    }

    /// The model flagged ambiguity — a candidate for a single Level-3 (thinking) retry.
    pub fn is_ambiguous(&self) -> bool {
        self.all_places().iter().any(|p| p.ambiguous == Some(true))
    }
}

/// A place as the AI understood it. Has no coordinates by design: the place provider supplies them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedPlace {
    pub display_name: String,
    #[serde(default)]
    pub alternative_names: Option<Vec<String>>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub place_confidence: Option<f64>,
    #[serde(default)]
    pub ambiguous: Option<bool>,
    #[serde(default)]
    pub search_query: Option<String>,
    #[serde(default)]
    pub has_useful_photo: Option<bool>,
    #[serde(default)]
    pub facts: Option<Vec<ExtractedFact>>,
}

impl ExtractedPlace {
    pub fn place_category(&self) -> PlaceCategory {
        PlaceCategory::fuzzy(self.category.as_deref()).unwrap_or(PlaceCategory::Other)
    }

    pub fn all_facts(&self) -> Vec<ExtractedFact> {
        self.facts.clone().unwrap_or_default().into_iter().filter(|f| !f.text.trim().is_empty()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names = vec![self.display_name.trim().to_string()];
        for n in self.alternative_names.clone().unwrap_or_default() {
            let n = n.trim().to_string();
            if !n.is_empty() && !names.contains(&n) {
                names.push(n);
            }
        }
        names
    }

    /// Natural-language map query, e.g. "Shibuya Sky, Tokyo, Japan".
    pub fn map_query(&self) -> String {
        if let Some(q) = self.search_query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            return q.to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        for p in [Some(&self.display_name), self.city.as_ref(), self.region.as_ref(), self.country.as_ref()]
            .into_iter()
            .flatten()
        {
            let p = p.trim().to_string();
            if !p.is_empty() && !parts.contains(&p) {
                parts.push(p);
            }
        }
        parts.join(", ")
    }

    /// A minimal extracted place for user-driven actions (e.g. "Add place" from the UI).
    pub fn named(name: &str) -> Self {
        Self {
            display_name: name.to_string(),
            alternative_names: None, city: None, region: None, country: None, category: None,
            place_confidence: Some(1.0), ambiguous: None, search_query: None, has_useful_photo: None, facts: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedFact {
    #[serde(default, rename = "type")]
    pub fact_type: Option<String>,
    pub text: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub source_quote: Option<String>,
    /// Indices into the numbered OCR lines sent to the model, mapped back to OCR block IDs.
    #[serde(default)]
    pub source_lines: Option<Vec<i64>>,
}

impl ExtractedFact {
    pub fn kind(&self) -> TravelFactType {
        TravelFactType::fuzzy(self.fact_type.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionClassification {
    pub region_type: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub is_useful_travel_photo: Option<bool>,
}

impl RegionClassification {
    pub fn kind(&self) -> RegionType {
        RegionType::parse(&self.region_type.to_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceSummary {
    pub summary: String,
    #[serde(default)]
    pub used_fact_ids: Option<Vec<String>>,
}

/// Input for screenshot-level requests: numbered OCR lines + minimal metadata.
#[derive(Debug, Clone, Default)]
pub struct ScreenshotAiInput {
    pub lines: Vec<String>,
    /// `line_block_ids[i]` is the OCR block behind line i.
    pub line_block_ids: Vec<String>,
    pub creation_date: Option<String>,
    pub source_hint: Option<String>,
    pub creator_hint: Option<String>,
    /// Compressed JPEG, attached only when visual context is genuinely needed.
    pub image_jpeg: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub model: String,
    pub thinking: bool,
    pub image_used: bool,
    pub input_tokens: u64,
    pub cache_hit_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub retries: u32,
    pub estimated_cost: f64,
}

#[derive(Debug, Clone)]
pub struct AiResult<T> {
    pub value: T,
    pub usage: AiUsage,
    /// Raw JSON, stored in `ai_cache`.
    pub raw_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("No DeepSeek API key is configured. Add DEEPSEEK_API_KEY to .env.local or save one in Settings.")]
    NotConfigured,
    #[error("No network connection — the screenshot will be processed when you're back online.")]
    Offline,
    #[error("DeepSeek rate limit reached")]
    RateLimited,
    #[error("Today's AI request limit has been reached")]
    DailyLimitReached,
    #[error("DeepSeek error {status}: {message}")]
    Http { status: u16, message: String },
    #[error("DeepSeek returned an unexpected response")]
    InvalidResponse,
    #[error("DeepSeek returned invalid JSON: {0}")]
    InvalidJson(String),
}

impl AiError {
    /// Errors that park the screenshot until later rather than failing it.
    pub fn is_transient(&self) -> bool {
        matches!(self, AiError::Offline | AiError::RateLimited | AiError::DailyLimitReached)
    }
}

/// Strict decoding of model output, tolerating a Markdown code fence.
pub fn decode_json<T: serde::de::DeserializeOwned>(content: &str) -> Result<T, AiError> {
    let mut text = content.trim();
    if text.starts_with("```") {
        text = text.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    }
    serde_json::from_str(text).map_err(|e| AiError::InvalidJson(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_full_extraction_with_multiple_places() {
        let json = r#"{
          "is_travel_related": true, "travel_confidence": 0.96, "source_type": "instagram",
          "creator": "@examplecreator",
          "places": [
            {"display_name": "Shibuya Sky", "alternative_names": [], "city": "Tokyo", "region": "Tokyo",
             "country": "Japan", "category": "viewpoint", "place_confidence": 0.98,
             "search_query": "Shibuya Sky Tokyo Japan", "has_useful_photo": true,
             "facts": [{"type": "recommended_time", "text": "Visit around sunset", "confidence": 0.91,
                        "source_quote": "Try to book around sunset", "source_lines": [2]}]},
            {"display_name": "Ichiran Shibuya", "city": "Tokyo", "country": "Japan", "category": "restaurant", "facts": []}
          ]}"#;
        let e: TravelExtraction = decode_json(json).unwrap();
        assert_eq!(e.all_places().len(), 2);
        let first = &e.all_places()[0];
        assert_eq!(first.place_category(), PlaceCategory::Viewpoint);
        assert_eq!(first.all_facts()[0].kind(), TravelFactType::RecommendedTime);
        assert_eq!(first.map_query(), "Shibuya Sky Tokyo Japan");
    }

    #[test]
    fn tolerates_nulls_and_missing_fields() {
        let json = r#"{"is_travel_related": true, "travel_confidence": 0.9, "creator": null,
          "places": [{"display_name": "Lake Bled", "city": null, "country": null, "category": null, "facts": null}]}"#;
        let e: TravelExtraction = decode_json(json).unwrap();
        let p = &e.all_places()[0];
        assert_eq!(p.place_category(), PlaceCategory::Other);
        assert!(p.all_facts().is_empty());
        assert_eq!(p.map_query(), "Lake Bled");
    }

    #[test]
    fn no_places_and_empty_names_are_dropped() {
        let e: TravelExtraction = decode_json(r#"{"is_travel_related": false, "travel_confidence": 0.1}"#).unwrap();
        assert!(e.all_places().is_empty());
        let e: TravelExtraction =
            decode_json(r#"{"is_travel_related": true, "places": [{"display_name": "  "}]}"#).unwrap();
        assert!(e.all_places().is_empty());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(decode_json::<TravelExtraction>("{not json"), Err(AiError::InvalidJson(_))));
        assert!(matches!(decode_json::<TravelExtraction>(r#"{"places": []}"#), Err(AiError::InvalidJson(_))));
        assert!(matches!(decode_json::<TravelExtraction>("Sure! Here is the JSON"), Err(AiError::InvalidJson(_))));
    }

    #[test]
    fn strips_code_fences() {
        let e: TravelClassification =
            decode_json("```json\n{\"is_travel_related\": true, \"confidence\": 0.7}\n```").unwrap();
        assert!(e.is_travel_related);
    }
}
