// Feature: veya-mvp, Property 15: 设置往返一致性
//
// For any valid set of settings values (including shortcut strings and locale
// codes), saving settings and reading them back should return exactly the same
// values. Language switching should be immediately reflected in the loaded
// settings.
//
// Validates: Requirements 7.5, 7.6, 9.2

use proptest::prelude::*;
use tempfile::TempDir;
use veya_lib::db::Database;
use veya_lib::settings::AppSettings;

/// Strategy for generating a valid locale string.
fn arb_locale() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("zh-CN".to_string()),
        Just("en-US".to_string()),
        Just("ja-JP".to_string()),
        Just("ko-KR".to_string()),
        Just("fr-FR".to_string()),
    ]
}

/// Strategy for generating a valid shortcut string.
fn arb_shortcut() -> impl Strategy<Value = String> {
    let modifiers = prop_oneof![
        Just("CommandOrControl"),
        Just("Ctrl"),
        Just("Alt"),
        Just("Shift"),
    ];
    let keys = prop_oneof![
        Just("S"),
        Just("X"),
        Just("C"),
        Just("P"),
        Just("F1"),
        Just("F12"),
    ];
    (modifiers, keys).prop_map(|(m, k)| format!("{m}+Shift+{k}"))
}

/// Strategy for generating a complete valid AppSettings.
fn arb_settings() -> impl Strategy<Value = AppSettings> {
    (
        any::<bool>(),           // ai_completion_enabled
        1u64..10_000,            // cache_max_size_mb
        1u32..365,               // cache_auto_clean_days
        1u32..20,                // retry_count
        arb_shortcut(),          // shortcut_capture
        arb_locale(),            // locale
    )
        .prop_map(|(ai, cache_mb, clean_days, retry, shortcut, locale)| {
            AppSettings {
                ai_completion_enabled: ai,
                cache_max_size_mb: cache_mb,
                cache_auto_clean_days: clean_days,
                retry_count: retry,
                shortcut_capture: shortcut,
                locale,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Saving settings and loading them back should return identical values.
    #[test]
    fn settings_roundtrip_preserves_all_fields(settings in arb_settings()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        settings.save(&db).unwrap();
        let loaded = AppSettings::load(&db).unwrap();

        prop_assert_eq!(loaded.ai_completion_enabled, settings.ai_completion_enabled);
        prop_assert_eq!(loaded.cache_max_size_mb, settings.cache_max_size_mb);
        prop_assert_eq!(loaded.cache_auto_clean_days, settings.cache_auto_clean_days);
        prop_assert_eq!(loaded.retry_count, settings.retry_count);
        prop_assert_eq!(&loaded.shortcut_capture, &settings.shortcut_capture);
        prop_assert_eq!(&loaded.locale, &settings.locale);
    }

    /// Switching locale and saving should immediately reflect in the next load.
    #[test]
    fn locale_switch_immediately_reflected(
        initial in arb_settings(),
        new_locale in arb_locale(),
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        // Save initial settings.
        initial.save(&db).unwrap();

        // Switch locale and save again.
        let mut updated = initial.clone();
        updated.locale = new_locale.clone();
        updated.save(&db).unwrap();

        let loaded = AppSettings::load(&db).unwrap();
        prop_assert_eq!(&loaded.locale, &new_locale);
        // Other fields should remain unchanged.
        prop_assert_eq!(loaded.ai_completion_enabled, initial.ai_completion_enabled);
        prop_assert_eq!(loaded.cache_max_size_mb, initial.cache_max_size_mb);
        prop_assert_eq!(loaded.retry_count, initial.retry_count);
    }

    /// Overwriting settings with new values should fully replace the old ones.
    #[test]
    fn settings_overwrite_replaces_all(
        first in arb_settings(),
        second in arb_settings(),
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        first.save(&db).unwrap();
        second.save(&db).unwrap();

        let loaded = AppSettings::load(&db).unwrap();
        prop_assert_eq!(loaded, second);
    }
}
