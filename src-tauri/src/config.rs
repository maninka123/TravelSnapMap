//! Every tunable of the pipeline. Persisted in the `settings` table and editable in the UI.

use serde::{Deserialize, Serialize};

/// Bumping these marks existing results as outdated so they can be re-run selectively.
pub const PROCESSING_VERSION: i64 = 1;
pub const OCR_VERSION: i64 = 1;
pub const AI_PROMPT_VERSION: i64 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Settings schema version; older saved settings are upgraded once on load.
    /// (Field-level default: settings saved before versioning existed read as 0, not the current version.)
    #[serde(default)]
    pub config_version: u32,
    // Model
    pub model: String,
    /// Chat-completions base URL. Point at a backend proxy in production so the DeepSeek
    /// secret never ships inside the app.
    pub base_url: String,
    pub use_thinking_by_default: bool,
    /// Level 3: retry a persistently ambiguous extraction once with thinking enabled.
    pub allow_thinking_escalation: bool,
    /// When false, DeepSeek only ever receives OCR text ("Never send screenshot images to AI").
    pub allow_vision_requests: bool,
    pub max_output_tokens: u32,
    pub thinking_max_output_tokens: u32,
    pub vision_image_max_pixel_size: u32,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    /// 0 = unlimited.
    pub daily_ai_request_limit: u32,

    // Level 1: local filtering
    /// Local travel score below which a screenshot never reaches the AI.
    pub local_skip_score: f64,
    /// Local "definitely not travel" confidence at which AI is skipped regardless of score.
    pub minimum_local_confidence_for_skipping_ai: f64,

    // Travel classification thresholds (AI probability that the screenshot is travel)
    pub minimum_ai_confidence_for_auto_acceptance: f64,
    pub travel_review_threshold: f64,

    // Place resolution thresholds
    pub place_auto_accept_score: f64,
    pub place_review_score: f64,

    // Photo regions
    pub region_auto_accept_confidence: f64,
    pub region_review_confidence: f64,
    /// Cosine distance between feature prints under which two crops are the same photo.
    pub duplicate_image_distance: f64,

    // Processing
    pub max_concurrent_screenshots: usize,
    pub scan_from_year: Option<i32>,
    pub scan_to_year: Option<i32>,
    pub nearby_radius_km: f64,
    /// Process screenshots as soon as Photos reports new ones (off by default).
    pub auto_process_new_screenshots: bool,
    /// Folders of screenshot images scanned alongside Apple Photos.
    pub screenshot_folders: Vec<String>,

    // Instagram Reels
    /// "auto" (detect from the Reel) or a locale such as ja-JP.
    pub transcription_locale: String,
    /// Key snapshots OCR'd per Reel (OCR never runs on every frame).
    pub max_keyframes: u32,
    /// Optional explicit path to yt-dlp; auto-detected when empty.
    pub yt_dlp_path: String,
    /// Browser whose cookies yt-dlp may use for Reels that need a login (empty = none).
    pub cookies_from_browser: String,

    // Cost estimate (USD per million tokens) — update from DeepSeek's pricing page.
    pub price_input_cache_miss_per_million: f64,
    pub price_input_cache_hit_per_million: f64,
    pub price_output_per_million: f64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            model: "deepseek-flash".into(),
            base_url: "https://api.deepseek.com".into(),
            use_thinking_by_default: false,
            allow_thinking_escalation: true,
            allow_vision_requests: true,
            // A cap, not a cost: billing is per generated token. Too low just wastes cut-off attempts.
            max_output_tokens: 1500,
            thinking_max_output_tokens: 3000,
            vision_image_max_pixel_size: 768,
            request_timeout_secs: 60,
            max_retries: 2,
            daily_ai_request_limit: 0,
            local_skip_score: 0.2,
            minimum_local_confidence_for_skipping_ai: 0.95,
            minimum_ai_confidence_for_auto_acceptance: 0.8,
            travel_review_threshold: 0.5,
            place_auto_accept_score: 0.75,
            place_review_score: 0.6,
            region_auto_accept_confidence: 0.75,
            region_review_confidence: 0.5,
            duplicate_image_distance: 0.08,
            max_concurrent_screenshots: 2,
            scan_from_year: None,
            scan_to_year: None,
            nearby_radius_km: 10.0,
            auto_process_new_screenshots: false,
            screenshot_folders: Vec::new(),
            transcription_locale: "auto".into(),
            max_keyframes: 8,
            yt_dlp_path: String::new(),
            cookies_from_browser: String::new(),
            price_input_cache_miss_per_million: 0.14,
            price_input_cache_hit_per_million: 0.028,
            price_output_per_million: 0.28,
        }
    }
}

pub const CONFIG_VERSION: u32 = 2;

impl AppConfig {
    /// Moves settings saved by older builds to the current defaults where the old default was
    /// measured to be wrong on real data; values the user changed are kept.
    pub fn upgraded(mut self) -> Self {
        if self.config_version < 2 {
            if matches!(self.max_output_tokens, 700 | 900) { self.max_output_tokens = 1500; }
            if (self.place_review_score - 0.45).abs() < 1e-9 { self.place_review_score = 0.6; }
            if self.transcription_locale == "en-US" { self.transcription_locale = "auto".into(); }
            if (self.duplicate_image_distance - 0.35).abs() < 1e-9 { self.duplicate_image_distance = 0.08; }
        }
        self.config_version = CONFIG_VERSION;
        self
    }

    pub fn estimated_cost(&self, cache_miss: u64, cache_hit: u64, output: u64) -> f64 {
        (cache_miss as f64 * self.price_input_cache_miss_per_million
            + cache_hit as f64 * self.price_input_cache_hit_per_million
            + output as f64 * self.price_output_per_million)
            / 1_000_000.0
    }

    /// Requests go straight to DeepSeek (so a key is needed locally) rather than via a proxy.
    pub fn is_direct_deepseek(&self) -> bool {
        self.base_url.contains("deepseek.com")
    }
}

/// Finds the DeepSeek key without it ever being committed or stored in plain text. Order:
/// 1. `DEEPSEEK_API_KEY` environment variable / `.env.local` (development; loaded at startup)
/// 2. macOS Keychain (saved from Settings)
pub fn resolve_api_key() -> (Option<String>, &'static str) {
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.trim().is_empty() {
            return (Some(k.trim().to_string()), "environment / .env.local");
        }
    }
    match crate::secrets::deepseek_key() {
        Some(k) => (Some(k), "macOS Keychain"),
        None => (None, "not configured"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_are_upgraded_but_user_choices_kept() {
        let old: AppConfig = serde_json::from_str(r#"{"maxOutputTokens": 700, "placeReviewScore": 0.45, "transcriptionLocale": "en-US", "model": "my-model"}"#).unwrap();
        assert_eq!(old.config_version, 0);
        let new = old.upgraded();
        assert_eq!((new.max_output_tokens, new.place_review_score, new.transcription_locale.as_str()), (1500, 0.6, "auto"));
        assert_eq!(new.model, "my-model");
        let custom: AppConfig = serde_json::from_str(r#"{"maxOutputTokens": 2500}"#).unwrap();
        assert_eq!(custom.upgraded().max_output_tokens, 2500);
    }
}
