use serde::Serialize;

use crate::api_config::ApiProvider;
use crate::error::VeyaError;
use crate::retry::RetryPolicy;

/// Configuration for a single TTS service endpoint.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub provider: ApiProvider,
    pub base_url: String,
    pub model_name: String,
    pub api_key: String,
    /// The language this config serves (e.g. "en", "zh").
    pub language: String,
}

/// Options for a TTS synthesis request.
#[derive(Debug, Clone, Serialize)]
pub struct TtsOptions {
    pub voice: Option<String>,
    pub speed: Option<f32>,
}

/// Unified TTS client that routes requests to the correct service by language.
pub struct TtsClient {
    configs: Vec<TtsConfig>,
    http_client: reqwest::Client,
    retry_policy: RetryPolicy,
}

impl TtsClient {
    pub fn new(configs: Vec<TtsConfig>, retry_policy: RetryPolicy) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self {
            configs,
            http_client,
            retry_policy,
        }
    }

    /// Synthesize text to audio bytes, routing to the TTS service
    /// configured for the given language code.
    pub async fn synthesize(
        &self,
        text: &str,
        language: &str,
        options: &TtsOptions,
    ) -> Result<Vec<u8>, VeyaError> {
        let config = self.find_config(language)?;
        let config_clone = config.clone();
        let client = self.http_client.clone();
        let text_owned = text.to_string();
        let opts = options.clone();

        self.retry_policy
            .execute(|| {
                let cfg = config_clone.clone();
                let cl = client.clone();
                let t = text_owned.clone();
                let o = opts.clone();
                async move { Self::synthesize_once(&cfg, &cl, &t, &o).await }
            })
            .await
    }

    /// Returns the TTS config for the given language.
    /// Falls back to the first available config if no exact match.
    pub fn find_config(&self, language: &str) -> Result<&TtsConfig, VeyaError> {
        // Exact match first
        if let Some(cfg) = self.configs.iter().find(|c| c.language == language) {
            return Ok(cfg);
        }
        // Try prefix match (e.g. "en" matches "en-US")
        if let Some(cfg) = self
            .configs
            .iter()
            .find(|c| language.starts_with(&c.language) || c.language.starts_with(language))
        {
            return Ok(cfg);
        }
        // Fallback to first config
        self.configs
            .first()
            .ok_or_else(|| VeyaError::TtsFailed("No TTS service configured".into()))
    }

    /// Returns the base_url of the TTS service for the given language.
    /// Useful for property testing language routing.
    pub fn route_url(&self, language: &str) -> Result<String, VeyaError> {
        self.find_config(language).map(|c| c.base_url.clone())
    }

    async fn synthesize_once(
        config: &TtsConfig,
        client: &reqwest::Client,
        text: &str,
        options: &TtsOptions,
    ) -> Result<Vec<u8>, VeyaError> {
        match config.provider {
            ApiProvider::Elevenlabs => {
                Self::synthesize_elevenlabs(config, client, text, options).await
            }
            // OpenAI-compatible TTS endpoint (OpenAI, Ollama, Custom)
            _ => Self::synthesize_openai(config, client, text, options).await,
        }
    }

    async fn synthesize_openai(
        config: &TtsConfig,
        client: &reqwest::Client,
        text: &str,
        options: &TtsOptions,
    ) -> Result<Vec<u8>, VeyaError> {
        let url = format!("{}/audio/speech", config.base_url.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "model": config.model_name,
            "input": text,
            "voice": options.voice.as_deref().unwrap_or("alloy"),
            "response_format": "mp3",
        });
        if let Some(speed) = options.speed {
            body["speed"] = serde_json::json!(speed);
        }

        let mut req = client.post(&url).json(&body);
        if !config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", config.api_key));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| Self::classify_error(e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Self::classify_http_status(status.as_u16(), &body_text));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| VeyaError::TtsFailed(format!("Failed to read audio: {e}")))?;

        Ok(bytes.to_vec())
    }

    async fn synthesize_elevenlabs(
        config: &TtsConfig,
        client: &reqwest::Client,
        text: &str,
        options: &TtsOptions,
    ) -> Result<Vec<u8>, VeyaError> {
        // ElevenLabs: POST /v1/text-to-speech/{voice_id}
        let voice = options.voice.as_deref().unwrap_or("21m00Tcm4TlvDq8ikWAM");
        let url = format!(
            "{}/v1/text-to-speech/{}",
            config.base_url.trim_end_matches('/'),
            voice
        );

        let mut body = serde_json::json!({
            "text": text,
            "model_id": config.model_name,
        });
        if let Some(speed) = options.speed {
            body["voice_settings"] = serde_json::json!({
                "stability": 0.5,
                "similarity_boost": 0.75,
                "speed": speed,
            });
        }

        let resp = client
            .post(&url)
            .header("xi-api-key", &config.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Self::classify_error(e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(Self::classify_http_status(status.as_u16(), &body_text));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| VeyaError::TtsFailed(format!("Failed to read audio: {e}")))?;

        Ok(bytes.to_vec())
    }

    fn classify_error(e: reqwest::Error) -> VeyaError {
        if e.is_timeout() {
            VeyaError::TtsFailed(format!("TTS request timed out: {e}"))
        } else if e.is_connect() {
            VeyaError::TtsFailed(format!("TTS connection failed: {e}"))
        } else {
            VeyaError::TtsFailed(format!("TTS request failed: {e}"))
        }
    }

    fn classify_http_status(status: u16, body: &str) -> VeyaError {
        match status {
            401 | 403 => VeyaError::InvalidApiKey(format!("TTS auth failed: {body}")),
            402 | 429 => VeyaError::InsufficientBalance(format!("TTS quota exceeded: {body}")),
            500..=599 => VeyaError::TtsFailed(format!("TTS server error ({status}): {body}")),
            _ => VeyaError::TtsFailed(format!("TTS HTTP {status}: {body}")),
        }
    }
}
