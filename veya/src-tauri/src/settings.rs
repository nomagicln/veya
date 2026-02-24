use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::Database;
use crate::error::VeyaError;

// ── AppSettings struct ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub ai_completion_enabled: bool,
    pub cache_max_size_mb: u64,
    pub cache_auto_clean_days: u32,
    pub retry_count: u32,
    pub shortcut_capture: String,
    pub locale: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_completion_enabled: true,
            cache_max_size_mb: 500,
            cache_auto_clean_days: 30,
            retry_count: 3,
            shortcut_capture: "CommandOrControl+Shift+S".into(),
            locale: "zh-CN".into(),
        }
    }
}

// Setting keys stored in the SQLite `settings` table.
const KEY_AI_COMPLETION: &str = "ai_completion_enabled";
const KEY_CACHE_MAX_SIZE: &str = "cache_max_size_mb";
const KEY_CACHE_CLEAN_DAYS: &str = "cache_auto_clean_days";
const KEY_RETRY_COUNT: &str = "retry_count";
const KEY_SHORTCUT_CAPTURE: &str = "shortcut_capture";
const KEY_LOCALE: &str = "locale";

impl AppSettings {
    /// Load settings from the database, falling back to defaults for missing keys.
    pub fn load(db: &Database) -> Result<Self, VeyaError> {
        let defaults = Self::default();

        let ai_completion_enabled = db
            .get_setting(KEY_AI_COMPLETION)?
            .map(|v| v == "true")
            .unwrap_or(defaults.ai_completion_enabled);

        let cache_max_size_mb = db
            .get_setting(KEY_CACHE_MAX_SIZE)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.cache_max_size_mb);

        let cache_auto_clean_days = db
            .get_setting(KEY_CACHE_CLEAN_DAYS)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.cache_auto_clean_days);

        let retry_count = db
            .get_setting(KEY_RETRY_COUNT)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.retry_count);

        let shortcut_capture = db
            .get_setting(KEY_SHORTCUT_CAPTURE)?
            .unwrap_or(defaults.shortcut_capture);

        let locale = db
            .get_setting(KEY_LOCALE)?
            .unwrap_or(defaults.locale);

        Ok(Self {
            ai_completion_enabled,
            cache_max_size_mb,
            cache_auto_clean_days,
            retry_count,
            shortcut_capture,
            locale,
        })
    }

    /// Persist all settings to the database.
    pub fn save(&self, db: &Database) -> Result<(), VeyaError> {
        db.set_setting(KEY_AI_COMPLETION, &self.ai_completion_enabled.to_string())?;
        db.set_setting(KEY_CACHE_MAX_SIZE, &self.cache_max_size_mb.to_string())?;
        db.set_setting(KEY_CACHE_CLEAN_DAYS, &self.cache_auto_clean_days.to_string())?;
        db.set_setting(KEY_RETRY_COUNT, &self.retry_count.to_string())?;
        db.set_setting(KEY_SHORTCUT_CAPTURE, &self.shortcut_capture)?;
        db.set_setting(KEY_LOCALE, &self.locale)?;
        Ok(())
    }
}

// ── Tauri Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<AppSettings, VeyaError> {
    AppSettings::load(&db)
}

#[tauri::command]
pub async fn update_settings(
    settings: AppSettings,
    db: tauri::State<'_, Arc<Database>>,
) -> Result<(), VeyaError> {
    settings.save(&db)
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
    fn load_returns_defaults_on_empty_db() {
        let (db, _dir) = test_db();
        let settings = AppSettings::load(&db).unwrap();
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (db, _dir) = test_db();
        let settings = AppSettings {
            ai_completion_enabled: false,
            cache_max_size_mb: 1024,
            cache_auto_clean_days: 7,
            retry_count: 5,
            shortcut_capture: "Ctrl+Alt+X".into(),
            locale: "en-US".into(),
        };
        settings.save(&db).unwrap();
        let loaded = AppSettings::load(&db).unwrap();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn partial_settings_fall_back_to_defaults() {
        let (db, _dir) = test_db();
        db.set_setting("locale", "en-US").unwrap();
        let loaded = AppSettings::load(&db).unwrap();
        assert_eq!(loaded.locale, "en-US");
        // Other fields should be defaults
        assert_eq!(loaded.ai_completion_enabled, true);
        assert_eq!(loaded.retry_count, 3);
    }
}
