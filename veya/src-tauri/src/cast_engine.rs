use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::api_config::{ApiConfig, ApiProvider, ModelType};
use crate::db::Database;
use crate::error::VeyaError;
use crate::llm_client::{LlmClient, LlmConfig, Message};
use crate::retry::RetryPolicy;
use crate::settings::AppSettings;
use crate::stronghold_store::StrongholdStore;
use crate::tts_client::{TtsClient, TtsConfig, TtsOptions};

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodcastSource {
    TextInsight,
    VisionCapture,
    Custom,
}

impl PodcastSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TextInsight => "text_insight",
            Self::VisionCapture => "vision_capture",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedMode {
    Slow,
    Normal,
}

impl SpeedMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Slow => "slow",
            Self::Normal => "normal",
        }
    }

    pub fn tts_speed(&self) -> f32 {
        match self {
            Self::Slow => 0.75,
            Self::Normal => 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodcastMode {
    Bilingual,
    Immersive,
}

impl PodcastMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bilingual => "bilingual",
            Self::Immersive => "immersive",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodcastInput {
    pub content: String,
    pub source: PodcastSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodcastOptions {
    pub speed: SpeedMode,
    pub mode: PodcastMode,
    pub target_language: String,
}

/// Progress event emitted to the frontend via `veya://cast-engine/progress`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastEngineProgress {
    #[serde(rename = "type")]
    pub progress_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const EVENT_PROGRESS: &str = "veya://cast-engine/progress";

// ── Helper: build prompt for script generation ───────────────────

fn build_script_prompt(input: &PodcastInput, options: &PodcastOptions) -> Vec<Message> {
    let mode_instruction = match options.mode {
        PodcastMode::Bilingual => {
            "Generate a bilingual podcast script. Alternate between the original language and the target language. \
             For each key phrase or sentence, first present it in the original language, then explain it in the target language."
        }
        PodcastMode::Immersive => {
            "Generate an immersive podcast script entirely in the target language. \
             Explain the content naturally as if teaching a language learner, using only the target language."
        }
    };

    let speed_instruction = match options.speed {
        SpeedMode::Slow => "Use short, simple sentences. Pause between ideas. Speak slowly and clearly.",
        SpeedMode::Normal => "Use natural conversational pace and sentence length.",
    };

    let system = format!(
        "You are a language learning podcast host. Your job is to transform the given content into \
         an engaging spoken explanation that helps learners understand the material.\n\n\
         Target language: {}\n\
         {}\n\
         {}\n\n\
         Output ONLY the podcast script text, ready to be read aloud. \
         Use paragraph breaks to separate segments. Do not include stage directions or metadata.",
        options.target_language, mode_instruction, speed_instruction
    );

    vec![
        Message {
            role: "system".into(),
            content: system,
        },
        Message {
            role: "user".into(),
            content: input.content.clone(),
        },
    ]
}

/// Split a script into segments for TTS synthesis.
/// Splits on double-newlines, falling back to single newlines, then by sentence.
pub fn split_script_segments(script: &str) -> Vec<String> {
    let segments: Vec<String> = script
        .split("\n\n")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.len() >= 2 {
        return segments;
    }

    // Fallback: split by single newline
    let segments: Vec<String> = script
        .split('\n')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        vec![script.trim().to_string()]
    } else {
        segments
    }
}

// ── Helper: resolve LLM and TTS clients from app state ───────────

fn resolve_llm_client(
    db: &Database,
    store: &StrongholdStore,
    retry_count: u32,
) -> Result<LlmClient, VeyaError> {
    let rows = db.get_api_configs()?;
    let text_row = rows
        .iter()
        .find(|r| r.model_type == "text")
        .ok_or_else(|| VeyaError::ModelUnavailable("No text model configured".into()))?;

    let config = ApiConfig::from_row(text_row)?;
    let api_key = store
        .get_api_key(&config.id)
        .unwrap_or_default()
        .unwrap_or_default();

    let llm_config = LlmConfig {
        provider: config.provider,
        base_url: config.base_url,
        model_name: config.model_name,
        api_key,
    };

    let retry = RetryPolicy::new(retry_count, 500, 30_000);
    Ok(LlmClient::new(llm_config, retry))
}

fn resolve_tts_client(
    db: &Database,
    store: &StrongholdStore,
    retry_count: u32,
) -> Result<TtsClient, VeyaError> {
    let rows = db.get_api_configs()?;
    let tts_rows: Vec<_> = rows.iter().filter(|r| r.model_type == "tts").collect();

    if tts_rows.is_empty() {
        return Err(VeyaError::TtsFailed("No TTS service configured".into()));
    }

    let mut configs = Vec::new();
    for row in tts_rows {
        let config = ApiConfig::from_row(row)?;
        let api_key = store
            .get_api_key(&config.id)
            .unwrap_or_default()
            .unwrap_or_default();

        configs.push(TtsConfig {
            provider: config.provider,
            base_url: config.base_url,
            model_name: config.model_name,
            api_key,
            language: config.language.unwrap_or_else(|| "en".into()),
        });
    }

    let retry = RetryPolicy::new(retry_count, 500, 30_000);
    Ok(TtsClient::new(configs, retry))
}

/// Ensure a directory exists, creating it if necessary.
fn ensure_dir(path: &PathBuf) -> Result<(), VeyaError> {
    std::fs::create_dir_all(path)
        .map_err(|e| VeyaError::StorageError(format!("Failed to create directory: {e}")))
}

/// Return the temp audio directory: `app_cache_dir()/audio/temp/`
pub fn temp_audio_dir(app: &AppHandle) -> Result<PathBuf, VeyaError> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| VeyaError::StorageError(format!("Failed to resolve cache dir: {e}")))?;
    Ok(cache.join("audio").join("temp"))
}

/// Return the saved audio directory: `app_data_dir()/audio/saved/`
pub fn saved_audio_dir(app: &AppHandle) -> Result<PathBuf, VeyaError> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| VeyaError::StorageError(format!("Failed to resolve data dir: {e}")))?;
    Ok(data.join("audio").join("saved"))
}

// ── Tauri Commands ───────────────────────────────────────────────

/// Generate a podcast from the given input. Returns the path to the temporary MP3 file.
///
/// Pipeline: script generation → segmentation → TTS synthesis → concatenation → MP3 output.
/// Progress is emitted via `veya://cast-engine/progress`.
#[tauri::command]
pub async fn generate_podcast(
    input: PodcastInput,
    options: PodcastOptions,
    app: AppHandle,
) -> Result<String, VeyaError> {
    let db = app.state::<Arc<Database>>();
    let store = app.state::<Arc<StrongholdStore>>();
    let settings = AppSettings::load(&db)?;

    // ── 1. Emit: script_generating ───────────────────────────────
    let _ = app.emit(
        EVENT_PROGRESS,
        CastEngineProgress {
            progress_type: "script_generating".into(),
            progress: Some(0),
            script_preview: None,
            audio_path: None,
            error: None,
        },
    );

    // ── 2. Generate script via LLM ───────────────────────────────
    let llm = resolve_llm_client(&db, &store, settings.retry_count)?;
    let messages = build_script_prompt(&input, &options);
    let script = llm.chat(messages).await?;

    // ── 3. Emit: script_done ─────────────────────────────────────
    let preview = if script.len() > 200 {
        format!("{}…", &script[..200])
    } else {
        script.clone()
    };
    let _ = app.emit(
        EVENT_PROGRESS,
        CastEngineProgress {
            progress_type: "script_done".into(),
            progress: Some(30),
            script_preview: Some(preview),
            audio_path: None,
            error: None,
        },
    );

    // ── 4. Split script into segments ────────────────────────────
    let segments = split_script_segments(&script);
    let total_segments = segments.len() as u32;

    // ── 5. TTS synthesis per segment ─────────────────────────────
    let tts = resolve_tts_client(&db, &store, settings.retry_count)?;
    let tts_options = TtsOptions {
        voice: None,
        speed: Some(options.speed.tts_speed()),
    };

    let mut all_audio: Vec<u8> = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let audio_bytes = tts
            .synthesize(segment, &options.target_language, &tts_options)
            .await?;
        all_audio.extend_from_slice(&audio_bytes);

        let pct = 30 + ((i as u32 + 1) * 60 / total_segments.max(1));
        let _ = app.emit(
            EVENT_PROGRESS,
            CastEngineProgress {
                progress_type: "tts_progress".into(),
                progress: Some(pct.min(90)),
                script_preview: None,
                audio_path: None,
                error: None,
            },
        );
    }

    // ── 6. Write concatenated audio to temp file ─────────────────
    let temp_dir = temp_audio_dir(&app)?;
    ensure_dir(&temp_dir)?;
    let filename = format!("{}.mp3", Uuid::new_v4());
    let file_path = temp_dir.join(&filename);

    std::fs::write(&file_path, &all_audio)
        .map_err(|e| VeyaError::StorageError(format!("Failed to write audio file: {e}")))?;

    let path_str = file_path.to_string_lossy().to_string();

    // ── 7. Emit: done ────────────────────────────────────────────
    let _ = app.emit(
        EVENT_PROGRESS,
        CastEngineProgress {
            progress_type: "done".into(),
            progress: Some(100),
            script_preview: None,
            audio_path: Some(path_str.clone()),
            error: None,
        },
    );

    Ok(path_str)
}

/// Save a temporary podcast audio to the persistent directory.
/// Returns the new persistent file path.
#[tauri::command]
pub async fn save_podcast(temp_path: String, app: AppHandle) -> Result<String, VeyaError> {
    let src = PathBuf::from(&temp_path);
    if !src.exists() {
        return Err(VeyaError::StorageError(format!(
            "Temp audio file not found: {temp_path}"
        )));
    }

    let saved_dir = saved_audio_dir(&app)?;
    ensure_dir(&saved_dir)?;

    let filename = src
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{}.mp3", Uuid::new_v4()));

    let dest = saved_dir.join(&filename);
    std::fs::copy(&src, &dest).map_err(|e| {
        VeyaError::StorageError(format!("Failed to copy audio to saved dir: {e}"))
    })?;

    Ok(dest.to_string_lossy().to_string())
}

/// Remove all files in the temporary audio cache directory.
#[tauri::command]
pub async fn cleanup_temp_audio(app: AppHandle) -> Result<(), VeyaError> {
    let temp_dir = temp_audio_dir(&app)?;
    if !temp_dir.exists() {
        return Ok(());
    }
    remove_dir_contents(&temp_dir)
}

/// Clean up saved audio files that exceed the configured max size or max age.
#[tauri::command]
pub async fn cleanup_saved_audio(app: AppHandle) -> Result<(), VeyaError> {
    let db = app.state::<Arc<Database>>();
    let settings = AppSettings::load(&db)?;
    let saved_dir = saved_audio_dir(&app)?;
    if !saved_dir.exists() {
        return Ok(());
    }

    cleanup_by_policy(
        &saved_dir,
        settings.cache_max_size_mb,
        settings.cache_auto_clean_days,
    )
}

// ── Internal helpers ─────────────────────────────────────────────

/// Remove all files inside a directory (but keep the directory itself).
fn remove_dir_contents(dir: &PathBuf) -> Result<(), VeyaError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| VeyaError::StorageError(format!("Failed to read dir: {e}")))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            std::fs::remove_file(&path).ok();
        }
    }
    Ok(())
}

/// Apply cache cleanup policy: remove files older than `max_days`, then remove
/// oldest files until total size is within `max_size_mb`.
pub fn cleanup_by_policy(
    dir: &PathBuf,
    max_size_mb: u64,
    max_days: u32,
) -> Result<(), VeyaError> {
    use std::time::{Duration, SystemTime};

    let max_age = Duration::from_secs(max_days as u64 * 86_400);
    let now = SystemTime::now();
    let max_bytes = max_size_mb * 1_024 * 1_024;

    // Collect file metadata
    let entries = std::fs::read_dir(dir)
        .map_err(|e| VeyaError::StorageError(format!("Failed to read dir: {e}")))?;

    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(meta) = path.metadata() {
            let modified = meta.modified().unwrap_or(now);
            files.push((path, meta.len(), modified));
        }
    }

    // Phase 1: remove files older than max_days
    files.retain(|(path, _, modified)| {
        if let Ok(age) = now.duration_since(*modified) {
            if age > max_age {
                std::fs::remove_file(path).ok();
                return false;
            }
        }
        true
    });

    // Phase 2: if still over budget, remove oldest files first
    let total_size: u64 = files.iter().map(|(_, sz, _)| sz).sum();
    if total_size > max_bytes {
        // Sort oldest first
        files.sort_by_key(|(_, _, modified)| *modified);
        let mut current = total_size;
        for (path, sz, _) in &files {
            if current <= max_bytes {
                break;
            }
            std::fs::remove_file(path).ok();
            current = current.saturating_sub(*sz);
        }
    }

    Ok(())
}
