//! DeepSeek (OpenAI-compatible chat completions) implementation of `TravelAIService`.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::prompts;
use super::types::*;
use super::TravelAIService;
use crate::config::AppConfig;

/// Thin HTTP client. Knows nothing about travel.
pub struct DeepSeekClient {
    http: reqwest::Client,
    config: AppConfig,
    api_key: Option<String>,
}

pub struct Completion {
    pub content: String,
    pub usage: AiUsage,
}

impl DeepSeekClient {
    pub fn new(config: AppConfig, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .expect("reqwest client");
        Self { http, config, api_key }
    }

    /// One JSON-mode request. Thinking is always set explicitly, never left to the API default.
    pub async fn complete_json(
        &self,
        system: &str,
        user: &str,
        image_jpeg: Option<&[u8]>,
        thinking: bool,
        max_tokens: u32,
    ) -> Result<Completion, AiError> {
        if self.config.is_direct_deepseek() && self.api_key.is_none() {
            return Err(AiError::NotConfigured);
        }
        let user_content: Value = match image_jpeg {
            Some(bytes) => json!([
                {"type": "text", "text": user},
                {"type": "image_url", "image_url": {
                    "url": format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
                }}
            ]),
            None => json!(user),
        };
        let body = json!({
            "model": self.config.model,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user_content}],
            "response_format": {"type": "json_object"},
            "thinking": {"type": if thinking { "enabled" } else { "disabled" }},
            "max_tokens": max_tokens,
            "temperature": 0,
            "stream": false,
        });
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let start = Instant::now();
        let mut attempt: u32 = 0;
        loop {
            let mut request = self.http.post(&url).json(&body);
            if let Some(key) = &self.api_key {
                request = request.bearer_auth(key);
            }
            let outcome = request.send().await;
            let retry_delay = Duration::from_millis(1500 * 2u64.pow(attempt));
            match outcome {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if status == 200 {
                        let json: Value = response.json().await.map_err(|_| AiError::InvalidResponse)?;
                        return self.parse(json, thinking, image_jpeg.is_some(), start.elapsed(), attempt);
                    }
                    if matches!(status, 429 | 500 | 502 | 503 | 504) && attempt < self.config.max_retries {
                        attempt += 1;
                        tokio::time::sleep(retry_delay).await;
                        continue;
                    }
                    if status == 429 {
                        return Err(AiError::RateLimited);
                    }
                    let text = response.text().await.unwrap_or_default();
                    return Err(AiError::Http { status, message: text.chars().take(200).collect() });
                }
                Err(e) if e.is_timeout() && attempt < self.config.max_retries => {
                    attempt += 1;
                    tokio::time::sleep(retry_delay).await;
                }
                Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => return Err(AiError::Offline),
                Err(_) => return Err(AiError::InvalidResponse),
            }
        }
    }

    fn parse(&self, json: Value, thinking: bool, image_used: bool, elapsed: Duration, retries: u32) -> Result<Completion, AiError> {
        let choice = &json["choices"][0];
        let content = choice["message"]["content"].as_str().ok_or(AiError::InvalidResponse)?.to_string();
        if choice["finish_reason"].as_str() == Some("length") {
            return Err(AiError::InvalidJson("response truncated at max_tokens".into()));
        }
        let usage = &json["usage"];
        let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0);
        let hit = usage["prompt_cache_hit_tokens"].as_u64().unwrap_or(0);
        let miss = usage["prompt_cache_miss_tokens"].as_u64().unwrap_or(prompt.saturating_sub(hit));
        let output = usage["completion_tokens"].as_u64().unwrap_or(0);
        Ok(Completion {
            content,
            usage: AiUsage {
                model: self.config.model.clone(),
                thinking,
                image_used,
                input_tokens: prompt,
                cache_hit_tokens: hit,
                output_tokens: output,
                latency_ms: elapsed.as_millis() as u64,
                retries,
                estimated_cost: self.config.estimated_cost(miss, hit, output),
            },
        })
    }
}

/// Builds travel prompts and validates every response against the Codable-style types.
pub struct DeepSeekTravelAIService {
    client: DeepSeekClient,
    config: AppConfig,
}

impl DeepSeekTravelAIService {
    pub fn new(config: AppConfig, api_key: Option<String>) -> Self {
        Self { client: DeepSeekClient::new(config.clone(), api_key), config }
    }

    fn max_tokens(&self, thinking: bool) -> u32 {
        if thinking { self.config.thinking_max_output_tokens } else { self.config.max_output_tokens }
    }

    /// Sends a request and decodes it; a response that fails validation is retried once
    /// (controlled retry policy), then reported as `InvalidJson` without touching any records.
    async fn request<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        image: Option<&[u8]>,
        thinking: bool,
    ) -> Result<AiResult<T>, AiError> {
        let mut total: Option<AiUsage> = None;
        let mut last_error = AiError::InvalidResponse;
        for _ in 0..2 {
            let completion = self.client.complete_json(system, user, image, thinking, self.max_tokens(thinking)).await?;
            let usage = match total.take() {
                Some(prev) => combine(prev, completion.usage),
                None => completion.usage,
            };
            match decode_json::<T>(&completion.content) {
                Ok(value) => return Ok(AiResult { value, usage, raw_json: completion.content }),
                Err(e) => {
                    log::warn!(target: "ai", "invalid JSON from model, retrying once: {e}");
                    last_error = e;
                    total = Some(AiUsage { retries: usage.retries + 1, ..usage });
                }
            }
        }
        Err(last_error)
    }
}

fn combine(a: AiUsage, b: AiUsage) -> AiUsage {
    AiUsage {
        model: b.model,
        thinking: a.thinking || b.thinking,
        image_used: a.image_used || b.image_used,
        input_tokens: a.input_tokens + b.input_tokens,
        cache_hit_tokens: a.cache_hit_tokens + b.cache_hit_tokens,
        output_tokens: a.output_tokens + b.output_tokens,
        latency_ms: a.latency_ms + b.latency_ms,
        retries: a.retries + b.retries,
        estimated_cost: a.estimated_cost + b.estimated_cost,
    }
}

#[async_trait]
impl TravelAIService for DeepSeekTravelAIService {
    async fn classify_screenshot(&self, input: &ScreenshotAiInput, thinking: bool) -> Result<AiResult<TravelClassification>, AiError> {
        let image = if self.config.allow_vision_requests { input.image_jpeg.as_deref() } else { None };
        self.request(prompts::CLASSIFICATION_SYSTEM, &prompts::screenshot_payload(input), image, thinking).await
    }

    async fn extract_travel_information(&self, input: &ScreenshotAiInput, thinking: bool) -> Result<AiResult<TravelExtraction>, AiError> {
        let image = if self.config.allow_vision_requests { input.image_jpeg.as_deref() } else { None };
        self.request(&prompts::extraction_system(), &prompts::screenshot_payload(input), image, thinking).await
    }

    async fn classify_image_region(&self, image_jpeg: &[u8], context: &str) -> Result<AiResult<RegionClassification>, AiError> {
        if !self.config.allow_vision_requests {
            return Err(AiError::InvalidResponse);
        }
        self.request(prompts::REGION_SYSTEM, context, Some(image_jpeg), false).await
    }

    async fn summarize_place(&self, name: &str, facts: &[(String, String)]) -> Result<AiResult<PlaceSummary>, AiError> {
        self.request(prompts::SUMMARY_SYSTEM, &prompts::summary_payload(name, facts), None, false).await
    }

    fn model_name(&self) -> String {
        self.config.model.clone()
    }
}
