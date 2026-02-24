use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{ApiConfigRow, Database};
use crate::error::VeyaError;
use crate::stronghold_store::StrongholdStore;

// ── Enums ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiProvider {
    Openai,
    Anthropic,
    Elevenlabs,
    Ollama,
    Custom,
}

impl ApiProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Elevenlabs => "elevenlabs",
            Self::Ollama => "ollama",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, VeyaError> {
        match s {
            "openai" => Ok(Self::Openai),
            "anthropic" => Ok(Self::Anthropic),
            "elevenlabs" => Ok(Self::Elevenlabs),
            "ollama" => Ok(Self::Ollama),
            "custom" => Ok(Self::Custom),
            _ => Err(VeyaError::StorageError(format!("Unknown provider: {s}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Text,
    Vision,
    Tts,
}

impl ModelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Vision => "vision",
            Self::Tts => "tts",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, VeyaError> {
        match s {
            "text" => Ok(Self::Text),
            "vision" => Ok(Self::Vision),
            "tts" => Ok(Self::Tts),
            _ => Err(VeyaError::StorageError(format!("Unknown model type: {s}"))),
        }
    }
}

// ── ApiConfig struct ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub id: String,
    pub name: String,
    pub provider: ApiProvider,
    pub model_type: ModelType,
    pub base_url: String,
    pub model_name: String,
    /// When saving, the caller sends the raw API key here.
    /// When reading, this field contains the Stronghold reference (not the plaintext key).
    #[serde(default)]
    pub api_key: Option<String>,
    /// Stronghold reference key — populated on read from DB.
    #[serde(default)]
    pub api_key_ref: Option<String>,
    /// Language binding for TTS configs.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl ApiConfig {
    /// Convert a database row into an ApiConfig (without the plaintext key).
    pub fn from_row(row: &ApiConfigRow) -> Result<Self, VeyaError> {
        Ok(Self {
            id: row.id.clone(),
            name: row.name.clone(),
            provider: ApiProvider::from_str(&row.provider)?,
            model_type: ModelType::from_str(&row.model_type)?,
            base_url: row.base_url.clone(),
            model_name: row.model_name.clone(),
            api_key: None,
            api_key_ref: Some(row.api_key_ref.clone()),
            language: row.language.clone(),
            is_local: row.is_local,
            is_active: row.is_active,
            created_at: Some(row.created_at.clone()),
        })
    }
}

// ── Tauri Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn get_api_configs(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<ApiConfig>, VeyaError> {
    let rows = db.get_api_configs()?;
    rows.iter().map(ApiConfig::from_row).collect()
}

#[tauri::command]
pub async fn save_api_config(
    config: ApiConfig,
    db: tauri::State<'_, Arc<Database>>,
    store: tauri::State<'_, Arc<StrongholdStore>>,
) -> Result<(), VeyaError> {
    let api_key_ref = format!("api_key_{}", config.id);

    // Store the API key in Stronghold if provided (skip for local models without keys).
    if let Some(ref key) = config.api_key {
        if !key.is_empty() {
            store.store_api_key(&config.id, key)?;
        }
    }

    // Persist metadata to SQLite.
    db.insert_api_config(
        &config.id,
        &config.name,
        config.provider.as_str(),
        config.model_type.as_str(),
        &config.base_url,
        &config.model_name,
        &api_key_ref,
        config.language.as_deref(),
        config.is_local,
    )?;

    Ok(())
}

#[tauri::command]
pub async fn delete_api_config_cmd(
    id: String,
    db: tauri::State<'_, Arc<Database>>,
    store: tauri::State<'_, Arc<StrongholdStore>>,
) -> Result<(), VeyaError> {
    // Remove from Stronghold first (ignore errors if key doesn't exist).
    let _ = store.delete_api_key(&id);
    db.delete_api_config(&id)?;
    Ok(())
}

#[tauri::command]
pub async fn test_api_connection(config: ApiConfig) -> Result<bool, VeyaError> {
    let api_key = config.api_key.clone().unwrap_or_default();

    // For local models (Ollama), just check if the endpoint is reachable.
    let url = if config.provider == ApiProvider::Ollama {
        format!("{}/api/tags", config.base_url.trim_end_matches('/'))
    } else {
        format!("{}/models", config.base_url.trim_end_matches('/'))
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| VeyaError::NetworkTimeout(format!("Failed to build HTTP client: {e}")))?;

    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => Ok(true),
        Ok(resp) if resp.status().as_u16() == 401 => {
            Err(VeyaError::InvalidApiKey("Authentication failed".into()))
        }
        Ok(resp) => Err(VeyaError::NetworkTimeout(format!(
            "Unexpected status: {}",
            resp.status()
        ))),
        Err(e) if e.is_timeout() => {
            Err(VeyaError::NetworkTimeout(format!("Connection timed out: {e}")))
        }
        Err(e) => Err(VeyaError::NetworkTimeout(format!("Connection failed: {e}"))),
    }
}
