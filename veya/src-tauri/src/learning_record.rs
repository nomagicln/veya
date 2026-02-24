use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{Database, PodcastRow, QueryRow, WordFreqRow};
use crate::error::VeyaError;

// ── Input types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveQueryInput {
    pub input_text: String,
    pub source: String,
    pub detected_language: Option<String>,
    pub analysis_result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavePodcastInput {
    pub input_content: String,
    pub source: String,
    pub speed_mode: String,
    pub podcast_mode: String,
    pub audio_file_path: String,
    pub duration_seconds: Option<i64>,
}

// ── Word tokenisation ────────────────────────────────────────────

/// Split text into words for frequency counting.
/// Uses Unicode-aware splitting: keeps alphabetic/numeric sequences and CJK
/// characters as individual tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if is_cjk(ch) {
            // Flush any accumulated alphabetic token first
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(ch.to_string());
        } else if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            current.push(ch);
        } else {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    // Normalise to lowercase for consistent counting
    tokens
        .into_iter()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF   |  // CJK Unified Ideographs
        0x3400..=0x4DBF   |  // CJK Extension A
        0x3000..=0x303F   |  // CJK Symbols and Punctuation
        0x3040..=0x309F   |  // Hiragana
        0x30A0..=0x30FF   |  // Katakana
        0xAC00..=0xD7AF      // Hangul Syllables
    )
}

// ── Core logic (testable without Tauri) ──────────────────────────

pub fn save_query(db: &Database, input: &SaveQueryInput) -> Result<QueryRow, VeyaError> {
    let id = Uuid::new_v4().to_string();
    let language = input.detected_language.as_deref();

    db.insert_query_record(&id, &input.input_text, &input.source, language, &input.analysis_result)?;

    // Update word frequency table
    let lang_code = language.unwrap_or("unknown");
    let words = tokenize(&input.input_text);
    for word in &words {
        db.increment_word_frequency(word, lang_code)?;
    }

    // Return the saved record
    let records = db.get_query_records(1, 1)?;
    records.into_iter().next().ok_or_else(|| {
        VeyaError::StorageError("Failed to retrieve saved query record".into())
    })
}

pub fn save_podcast(db: &Database, input: &SavePodcastInput) -> Result<PodcastRow, VeyaError> {
    let id = Uuid::new_v4().to_string();

    db.insert_podcast_record(
        &id,
        &input.input_content,
        &input.source,
        &input.speed_mode,
        &input.podcast_mode,
        &input.audio_file_path,
        input.duration_seconds,
    )?;

    let records = db.get_podcast_records(1, 1)?;
    records.into_iter().next().ok_or_else(|| {
        VeyaError::StorageError("Failed to retrieve saved podcast record".into())
    })
}

// ── Tauri Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn save_query_record(
    input: SaveQueryInput,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<QueryRow, VeyaError> {
    save_query(&db, &input)
}

#[tauri::command]
pub async fn save_podcast_record(
    input: SavePodcastInput,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<PodcastRow, VeyaError> {
    save_podcast(&db, &input)
}

#[tauri::command]
pub async fn get_query_history(
    page: u32,
    page_size: u32,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<QueryRow>, VeyaError> {
    db.get_query_records(page, page_size)
}

#[tauri::command]
pub async fn get_podcast_history(
    page: u32,
    page_size: u32,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<PodcastRow>, VeyaError> {
    db.get_podcast_records(page, page_size)
}

#[tauri::command]
pub async fn get_frequent_words(
    limit: u32,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<WordFreqRow>, VeyaError> {
    db.get_frequent_words(limit)
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();
        (db, dir)
    }

    #[test]
    fn tokenize_english() {
        let tokens = tokenize("Hello, world! It's a test.");
        assert_eq!(tokens, vec!["hello", "world", "it's", "a", "test"]);
    }

    #[test]
    fn tokenize_cjk() {
        let tokens = tokenize("你好世界");
        assert_eq!(tokens, vec!["你", "好", "世", "界"]);
    }

    #[test]
    fn tokenize_mixed() {
        let tokens = tokenize("Hello你好world");
        assert_eq!(tokens, vec!["hello", "你", "好", "world"]);
    }

    #[test]
    fn tokenize_empty() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("   ").is_empty());
    }

    #[test]
    fn save_query_creates_record_and_updates_frequency() {
        let (db, _dir) = test_db();
        let input = SaveQueryInput {
            input_text: "hello world hello".into(),
            source: "text_insight".into(),
            detected_language: Some("en".into()),
            analysis_result: r#"{"original":"hello world hello"}"#.into(),
        };

        let record = save_query(&db, &input).unwrap();
        assert_eq!(record.input_text, "hello world hello");
        assert_eq!(record.source, "text_insight");

        // Check word frequencies
        let words = db.get_frequent_words(10).unwrap();
        let hello = words.iter().find(|w| w.word == "hello").unwrap();
        assert_eq!(hello.count, 2);
        let world = words.iter().find(|w| w.word == "world").unwrap();
        assert_eq!(world.count, 1);
    }

    #[test]
    fn save_podcast_creates_record() {
        let (db, _dir) = test_db();
        let input = SavePodcastInput {
            input_content: "test content".into(),
            source: "custom".into(),
            speed_mode: "normal".into(),
            podcast_mode: "bilingual".into(),
            audio_file_path: "/tmp/test.mp3".into(),
            duration_seconds: Some(120),
        };

        let record = save_podcast(&db, &input).unwrap();
        assert_eq!(record.input_content, "test content");
        assert_eq!(record.speed_mode, "normal");
        assert_eq!(record.duration_seconds, Some(120));
    }

    #[test]
    fn query_history_pagination() {
        let (db, _dir) = test_db();
        for i in 0..5 {
            let input = SaveQueryInput {
                input_text: format!("query {i}"),
                source: "text_insight".into(),
                detected_language: None,
                analysis_result: "{}".into(),
            };
            save_query(&db, &input).unwrap();
        }

        let page1 = db.get_query_records(1, 2).unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db.get_query_records(2, 2).unwrap();
        assert_eq!(page2.len(), 2);
        let page3 = db.get_query_records(3, 2).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn frequent_words_ordered_by_count() {
        let (db, _dir) = test_db();
        // "hello" appears 3 times, "world" 1 time
        for input_text in &["hello hello hello", "world"] {
            let input = SaveQueryInput {
                input_text: input_text.to_string(),
                source: "text_insight".into(),
                detected_language: Some("en".into()),
                analysis_result: "{}".into(),
            };
            save_query(&db, &input).unwrap();
        }

        let words = db.get_frequent_words(10).unwrap();
        assert!(words.len() >= 2);
        assert_eq!(words[0].word, "hello");
        assert_eq!(words[0].count, 3);
    }
}
