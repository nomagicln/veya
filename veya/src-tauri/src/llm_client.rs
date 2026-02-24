use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::Emitter;

use crate::api_config::ApiProvider;
use crate::error::VeyaError;
use crate::retry::RetryPolicy;

// ── Message types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Configuration needed to make LLM requests.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: ApiProvider,
    pub base_url: String,
    pub model_name: String,
    pub api_key: String,
}

/// A chunk emitted during streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ── OpenAI-compatible request/response types ─────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Clone, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

// Anthropic uses a different request format
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

// Anthropic non-streaming response
#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

// ── LlmClient ────────────────────────────────────────────────────

pub struct LlmClient {
    config: LlmConfig,
    http_client: reqwest::Client,
    retry_policy: RetryPolicy,
}

impl LlmClient {
    pub fn new(config: LlmConfig, retry_policy: RetryPolicy) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            config,
            http_client,
            retry_policy,
        }
    }

    /// Non-streaming chat: returns the full response text.
    pub async fn chat(&self, messages: Vec<Message>) -> Result<String, VeyaError> {
        let config = self.config.clone();
        let client = self.http_client.clone();
        let msgs = messages.clone();

        self.retry_policy
            .execute(|| {
                let config = config.clone();
                let client = client.clone();
                let msgs = msgs.clone();
                async move { Self::chat_once(&config, &client, &msgs).await }
            })
            .await
    }

    /// Streaming chat: emits StreamChunk events via Tauri Event system.
    pub async fn stream_chat(
        &self,
        messages: Vec<Message>,
        app: &AppHandle,
        event_name: &str,
    ) -> Result<(), VeyaError> {
        // Emit start
        let _ = app.emit(
            event_name,
            StreamChunk {
                chunk_type: "start".into(),
                content: None,
            },
        );

        let result = self.stream_chat_inner(messages, app, event_name).await;

        match &result {
            Ok(()) => {
                let _ = app.emit(
                    event_name,
                    StreamChunk {
                        chunk_type: "done".into(),
                        content: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    event_name,
                    StreamChunk {
                        chunk_type: "error".into(),
                        content: Some(e.to_string()),
                    },
                );
            }
        }

        result
    }

    // ── Internal helpers ──────────────────────────────────────────

    async fn chat_once(
        config: &LlmConfig,
        client: &reqwest::Client,
        messages: &[Message],
    ) -> Result<String, VeyaError> {
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        match config.provider {
            ApiProvider::Anthropic => {
                Self::chat_once_anthropic(config, client, &chat_messages).await
            }
            // OpenAI, Ollama, ElevenLabs, Custom all use OpenAI-compatible format
            _ => Self::chat_once_openai(config, client, &chat_messages).await,
        }
    }

    async fn chat_once_openai(
        config: &LlmConfig,
        client: &reqwest::Client,
        messages: &[ChatMessage],
    ) -> Result<String, VeyaError> {
        let url = format!(
            "{}/chat/completions",
            config.base_url.trim_end_matches('/')
        );
        let body = ChatRequest {
            model: config.model_name.clone(),
            messages: messages.to_vec(),
            stream: false,
        };

        let mut req = client.post(&url).json(&body);
        if !config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", config.api_key));
        }

        let resp = req.send().await.map_err(|e| Self::classify_reqwest_error(e))?;
        let status = resp.status();

        if !status.is_success() {
            return Err(Self::classify_http_status(status.as_u16(), &resp.text().await.unwrap_or_default()));
        }

        let data: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| VeyaError::ModelUnavailable(format!("Invalid response: {e}")))?;

        data.choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| VeyaError::ModelUnavailable("Empty response from model".into()))
    }

    async fn chat_once_anthropic(
        config: &LlmConfig,
        client: &reqwest::Client,
        messages: &[ChatMessage],
    ) -> Result<String, VeyaError> {
        let url = format!(
            "{}/messages",
            config.base_url.trim_end_matches('/')
        );
        let body = AnthropicRequest {
            model: config.model_name.clone(),
            max_tokens: 4096,
            messages: messages.to_vec(),
            stream: false,
        };

        let resp = client
            .post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Self::classify_reqwest_error(e))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(Self::classify_http_status(status.as_u16(), &resp.text().await.unwrap_or_default()));
        }

        let data: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| VeyaError::ModelUnavailable(format!("Invalid Anthropic response: {e}")))?;

        data.content
            .first()
            .map(|c| c.text.clone())
            .ok_or_else(|| VeyaError::ModelUnavailable("Empty Anthropic response".into()))
    }

    /// Internal streaming implementation (without start/done envelope).
    async fn stream_chat_inner(
        &self,
        messages: Vec<Message>,
        app: &AppHandle,
        event_name: &str,
    ) -> Result<(), VeyaError> {
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        match self.config.provider {
            ApiProvider::Anthropic => {
                self.stream_anthropic(&chat_messages, app, event_name).await
            }
            _ => {
                self.stream_openai(&chat_messages, app, event_name).await
            }
        }
    }

    async fn stream_openai(
        &self,
        messages: &[ChatMessage],
        app: &AppHandle,
        event_name: &str,
    ) -> Result<(), VeyaError> {
        use futures_util::StreamExt;

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = ChatRequest {
            model: self.config.model_name.clone(),
            messages: messages.to_vec(),
            stream: true,
        };

        let mut req = self.http_client.post(&url).json(&body);
        if !self.config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        let resp = req.send().await.map_err(|e| Self::classify_reqwest_error(e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Self::classify_http_status(status.as_u16(), &resp.text().await.unwrap_or_default()));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| VeyaError::NetworkTimeout(format!("Stream error: {e}")))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            // Process complete SSE lines
            while let Some(pos) = buffer.find("\n\n") {
                let event_block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in event_block.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
                            return Ok(());
                        }
                        if let Some(content) = Self::parse_openai_sse_delta(data) {
                            let _ = app.emit(
                                event_name,
                                StreamChunk {
                                    chunk_type: "delta".into(),
                                    content: Some(content),
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn stream_anthropic(
        &self,
        messages: &[ChatMessage],
        app: &AppHandle,
        event_name: &str,
    ) -> Result<(), VeyaError> {
        use futures_util::StreamExt;

        let url = format!(
            "{}/messages",
            self.config.base_url.trim_end_matches('/')
        );
        let body = AnthropicRequest {
            model: self.config.model_name.clone(),
            max_tokens: 4096,
            messages: messages.to_vec(),
            stream: true,
        };

        let resp = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Self::classify_reqwest_error(e))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(Self::classify_http_status(status.as_u16(), &resp.text().await.unwrap_or_default()));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| VeyaError::NetworkTimeout(format!("Stream error: {e}")))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find("\n\n") {
                let event_block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in event_block.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Some(content) = Self::parse_anthropic_sse_delta(data) {
                            let _ = app.emit(
                                event_name,
                                StreamChunk {
                                    chunk_type: "delta".into(),
                                    content: Some(content),
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // ── SSE parsing helpers ───────────────────────────────────────

    fn parse_openai_sse_delta(data: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(data).ok()?;
        v.get("choices")?
            .get(0)?
            .get("delta")?
            .get("content")?
            .as_str()
            .map(|s| s.to_string())
    }

    fn parse_anthropic_sse_delta(data: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(data).ok()?;
        // Anthropic SSE: event type "content_block_delta" has delta.text
        if v.get("type")?.as_str()? == "content_block_delta" {
            return v.get("delta")?.get("text")?.as_str().map(|s| s.to_string());
        }
        None
    }

    // ── Error classification ──────────────────────────────────────

    fn classify_reqwest_error(e: reqwest::Error) -> VeyaError {
        if e.is_timeout() {
            VeyaError::NetworkTimeout(format!("Request timed out: {e}"))
        } else if e.is_connect() {
            VeyaError::NetworkTimeout(format!("Connection failed: {e}"))
        } else {
            VeyaError::ModelUnavailable(format!("Request failed: {e}"))
        }
    }

    fn classify_http_status(status: u16, body: &str) -> VeyaError {
        match status {
            401 => VeyaError::InvalidApiKey(format!("Authentication failed: {body}")),
            402 | 429 => {
                // 429 can mean rate limit or insufficient quota
                let lower = body.to_lowercase();
                if lower.contains("insufficient") || lower.contains("quota") || lower.contains("balance") {
                    VeyaError::InsufficientBalance(format!("Quota exceeded: {body}"))
                } else {
                    VeyaError::NetworkTimeout(format!("Rate limited: {body}"))
                }
            }
            403 => VeyaError::InvalidApiKey(format!("Forbidden: {body}")),
            404 => VeyaError::ModelUnavailable(format!("Model not found: {body}")),
            500..=599 => VeyaError::ModelUnavailable(format!("Server error ({status}): {body}")),
            _ => VeyaError::ModelUnavailable(format!("HTTP {status}: {body}")),
        }
    }
}
