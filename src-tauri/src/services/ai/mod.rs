//! The only path to an LLM. Commands and the UI never call DeepSeek directly: the processing
//! pipeline receives a `TravelAIService` by dependency injection (mocked in tests).

pub mod deepseek;
pub mod local_filter;
pub mod prompts;
pub mod types;

use async_trait::async_trait;

use types::*;

#[async_trait]
pub trait TravelAIService: Send + Sync {
    async fn classify_screenshot(&self, input: &ScreenshotAiInput, thinking: bool)
        -> Result<AiResult<TravelClassification>, AiError>;

    /// Combined classification + extraction in one request (the normal Level-2 call).
    async fn extract_travel_information(&self, input: &ScreenshotAiInput, thinking: bool)
        -> Result<AiResult<TravelExtraction>, AiError>;

    async fn classify_image_region(&self, image_jpeg: &[u8], context: &str)
        -> Result<AiResult<RegionClassification>, AiError>;

    /// Summary built only from the user's saved facts.
    async fn summarize_place(&self, name: &str, facts: &[(String, String)])
        -> Result<AiResult<PlaceSummary>, AiError>;

    fn model_name(&self) -> String;
}
