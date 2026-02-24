use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::VeyaError;

/// Core database wrapper providing SQLite access and migrations.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the database at `app_data_dir/veya.db` and run migrations.
    pub fn open(app_data_dir: PathBuf) -> Result<Self, VeyaError> {
        std::fs::create_dir_all(&app_data_dir).map_err(|e| {
            VeyaError::StorageError(format!("Failed to create data dir: {e}"))
        })?;

        let db_path = app_data_dir.join("veya.db");
        let conn = Connection::open(&db_path).map_err(|e| {
            VeyaError::StorageError(format!("Failed to open database: {e}"))
        })?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run all schema migrations.
    fn run_migrations(&self) -> Result<(), VeyaError> {
        let conn = self.conn.lock().map_err(|e| {
            VeyaError::StorageError(format!("Lock poisoned: {e}"))
        })?;

        conn.execute_batch(MIGRATION_V1).map_err(|e| {
            VeyaError::StorageError(format!("Migration failed: {e}"))
        })?;

        Ok(())
    }

    // ── Generic helpers ──────────────────────────────────────────────

    /// Execute a closure with an exclusive lock on the connection.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T, VeyaError>
    where
        F: FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    {
        let conn = self.conn.lock().map_err(|e| {
            VeyaError::StorageError(format!("Lock poisoned: {e}"))
        })?;
        f(&conn).map_err(|e| VeyaError::StorageError(e.to_string()))
    }

    // ── Settings helpers ─────────────────────────────────────────────

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, VeyaError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get(0)?)),
                None => Ok(None),
            }
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value],
            )?;
            Ok(())
        })
    }

    // ── Query record helpers ─────────────────────────────────────────

    pub fn insert_query_record(
        &self,
        id: &str,
        input_text: &str,
        source: &str,
        detected_language: Option<&str>,
        analysis_result: &str,
    ) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO query_records (id, input_text, source, detected_language, analysis_result)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, input_text, source, detected_language, analysis_result],
            )?;
            Ok(())
        })
    }

    pub fn get_query_records(&self, page: u32, page_size: u32) -> Result<Vec<QueryRow>, VeyaError> {
        self.with_conn(|conn| {
            let offset = page.saturating_sub(1) * page_size;
            let mut stmt = conn.prepare(
                "SELECT id, input_text, source, detected_language, analysis_result, created_at
                 FROM query_records ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![page_size, offset], |row| {
                Ok(QueryRow {
                    id: row.get(0)?,
                    input_text: row.get(1)?,
                    source: row.get(2)?,
                    detected_language: row.get(3)?,
                    analysis_result: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    // ── Podcast record helpers ───────────────────────────────────────

    pub fn insert_podcast_record(
        &self,
        id: &str,
        input_content: &str,
        source: &str,
        speed_mode: &str,
        podcast_mode: &str,
        audio_file_path: &str,
        duration_seconds: Option<i64>,
    ) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO podcast_records (id, input_content, source, speed_mode, podcast_mode, audio_file_path, duration_seconds)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, input_content, source, speed_mode, podcast_mode, audio_file_path, duration_seconds],
            )?;
            Ok(())
        })
    }

    pub fn get_podcast_records(&self, page: u32, page_size: u32) -> Result<Vec<PodcastRow>, VeyaError> {
        self.with_conn(|conn| {
            let offset = page.saturating_sub(1) * page_size;
            let mut stmt = conn.prepare(
                "SELECT id, input_content, source, speed_mode, podcast_mode, audio_file_path, duration_seconds, created_at
                 FROM podcast_records ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![page_size, offset], |row| {
                Ok(PodcastRow {
                    id: row.get(0)?,
                    input_content: row.get(1)?,
                    source: row.get(2)?,
                    speed_mode: row.get(3)?,
                    podcast_mode: row.get(4)?,
                    audio_file_path: row.get(5)?,
                    duration_seconds: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    // ── Word frequency helpers ───────────────────────────────────────

    pub fn increment_word_frequency(&self, word: &str, language: &str) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO word_frequency (word, language, count, last_queried_at)
                 VALUES (?1, ?2, 1, datetime('now'))
                 ON CONFLICT(word) DO UPDATE SET count = count + 1, last_queried_at = datetime('now')",
                params![word, language],
            )?;
            Ok(())
        })
    }

    pub fn get_frequent_words(&self, limit: u32) -> Result<Vec<WordFreqRow>, VeyaError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT word, language, count, last_queried_at
                 FROM word_frequency ORDER BY count DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok(WordFreqRow {
                    word: row.get(0)?,
                    language: row.get(1)?,
                    count: row.get(2)?,
                    last_queried_at: row.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    // ── API config helpers ───────────────────────────────────────────

    pub fn insert_api_config(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        model_type: &str,
        base_url: &str,
        model_name: &str,
        api_key_ref: &str,
        language: Option<&str>,
        is_local: bool,
    ) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO api_configs (id, name, provider, model_type, base_url, model_name, api_key_ref, language, is_local)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   name=excluded.name, provider=excluded.provider, model_type=excluded.model_type,
                   base_url=excluded.base_url, model_name=excluded.model_name, api_key_ref=excluded.api_key_ref,
                   language=excluded.language, is_local=excluded.is_local",
                params![id, name, provider, model_type, base_url, model_name, api_key_ref, language, is_local as i32],
            )?;
            Ok(())
        })
    }

    pub fn get_api_configs(&self) -> Result<Vec<ApiConfigRow>, VeyaError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, provider, model_type, base_url, model_name, api_key_ref, language, is_local, is_active, created_at
                 FROM api_configs ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ApiConfigRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider: row.get(2)?,
                    model_type: row.get(3)?,
                    base_url: row.get(4)?,
                    model_name: row.get(5)?,
                    api_key_ref: row.get(6)?,
                    language: row.get(7)?,
                    is_local: row.get::<_, i32>(8)? != 0,
                    is_active: row.get::<_, i32>(9)? != 0,
                    created_at: row.get(10)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn delete_api_config(&self, id: &str) -> Result<(), VeyaError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM api_configs WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}


// ── Row types ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryRow {
    pub id: String,
    pub input_text: String,
    pub source: String,
    pub detected_language: Option<String>,
    pub analysis_result: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PodcastRow {
    pub id: String,
    pub input_content: String,
    pub source: String,
    pub speed_mode: String,
    pub podcast_mode: String,
    pub audio_file_path: String,
    pub duration_seconds: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WordFreqRow {
    pub word: String,
    pub language: String,
    pub count: i64,
    pub last_queried_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiConfigRow {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_type: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key_ref: String,
    pub language: Option<String>,
    pub is_local: bool,
    pub is_active: bool,
    pub created_at: String,
}

// ── Migration SQL ────────────────────────────────────────────────

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS query_records (
    id TEXT PRIMARY KEY,
    input_text TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('text_insight', 'vision_capture')),
    detected_language TEXT,
    analysis_result TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS podcast_records (
    id TEXT PRIMARY KEY,
    input_content TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('text_insight', 'vision_capture', 'custom')),
    speed_mode TEXT NOT NULL CHECK(speed_mode IN ('slow', 'normal')),
    podcast_mode TEXT NOT NULL CHECK(podcast_mode IN ('bilingual', 'immersive')),
    audio_file_path TEXT NOT NULL,
    duration_seconds INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS word_frequency (
    word TEXT PRIMARY KEY,
    language TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 1,
    last_queried_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS api_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    model_type TEXT NOT NULL CHECK(model_type IN ('text', 'vision', 'tts')),
    base_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    api_key_ref TEXT NOT NULL,
    language TEXT,
    is_local INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

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
    fn migrations_create_all_tables() {
        let (db, _dir) = test_db();
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            )?;
            let tables: Vec<String> = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            assert!(tables.contains(&"query_records".to_string()));
            assert!(tables.contains(&"podcast_records".to_string()));
            assert!(tables.contains(&"word_frequency".to_string()));
            assert!(tables.contains(&"api_configs".to_string()));
            assert!(tables.contains(&"settings".to_string()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn settings_roundtrip() {
        let (db, _dir) = test_db();
        db.set_setting("locale", "zh-CN").unwrap();
        assert_eq!(db.get_setting("locale").unwrap(), Some("zh-CN".to_string()));
        db.set_setting("locale", "en-US").unwrap();
        assert_eq!(db.get_setting("locale").unwrap(), Some("en-US".to_string()));
    }

    #[test]
    fn query_record_insert_and_fetch() {
        let (db, _dir) = test_db();
        db.insert_query_record("q1", "hello", "text_insight", Some("en"), "{}").unwrap();
        let records = db.get_query_records(1, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "q1");
    }

    #[test]
    fn word_frequency_increment() {
        let (db, _dir) = test_db();
        db.increment_word_frequency("hello", "en").unwrap();
        db.increment_word_frequency("hello", "en").unwrap();
        let words = db.get_frequent_words(10).unwrap();
        assert_eq!(words[0].word, "hello");
        assert_eq!(words[0].count, 2);
    }

    #[test]
    fn api_config_crud() {
        let (db, _dir) = test_db();
        db.insert_api_config("c1", "GPT-4", "openai", "text", "https://api.openai.com", "gpt-4", "ref_c1", None, false).unwrap();
        let configs = db.get_api_configs().unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].api_key_ref, "ref_c1");
        db.delete_api_config("c1").unwrap();
        assert_eq!(db.get_api_configs().unwrap().len(), 0);
    }
}
