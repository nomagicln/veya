// Feature: veya-mvp, Property 12: 学习记录自动持久化
//
// For any completed query operation (text insight or vision capture) or podcast
// save operation, the database should contain a corresponding record with all
// required fields (input content, analysis result / audio path, timestamp).
//
// Validates: Requirements 6.1, 6.2

use proptest::prelude::*;
use tempfile::TempDir;
use veya_lib::db::Database;
use veya_lib::learning_record::{save_podcast, save_query, SavePodcastInput, SaveQueryInput};

/// Strategy for generating a valid query source.
fn arb_query_source() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("text_insight".to_string()),
        Just("vision_capture".to_string()),
    ]
}

/// Strategy for generating a valid podcast source.
fn arb_podcast_source() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("text_insight".to_string()),
        Just("vision_capture".to_string()),
        Just("custom".to_string()),
    ]
}

/// Strategy for generating an optional detected language.
fn arb_language() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("en".to_string())),
        Just(Some("zh".to_string())),
        Just(Some("ja".to_string())),
    ]
}

/// Strategy for generating a non-empty text string.
fn arb_nonempty_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ]{1,100}".prop_filter("must not be empty", |s| !s.trim().is_empty())
}

/// Strategy for generating a valid speed mode.
fn arb_speed_mode() -> impl Strategy<Value = String> {
    prop_oneof![Just("slow".to_string()), Just("normal".to_string()),]
}

/// Strategy for generating a valid podcast mode.
fn arb_podcast_mode() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("bilingual".to_string()),
        Just("immersive".to_string()),
    ]
}

/// Strategy for generating a SaveQueryInput.
fn arb_query_input() -> impl Strategy<Value = SaveQueryInput> {
    (
        arb_nonempty_text(),
        arb_query_source(),
        arb_language(),
        arb_nonempty_text(),
    )
        .prop_map(|(input_text, source, detected_language, analysis_result)| {
            let analysis_result = format!(r#"{{"original":"{analysis_result}"}}"#);
            SaveQueryInput {
                input_text,
                source,
                detected_language,
                analysis_result,
            }
        })
}

/// Strategy for generating a SavePodcastInput.
fn arb_podcast_input() -> impl Strategy<Value = SavePodcastInput> {
    (
        arb_nonempty_text(),
        arb_podcast_source(),
        arb_speed_mode(),
        arb_podcast_mode(),
        arb_nonempty_text(),
        prop_oneof![Just(None), (1i64..3600).prop_map(Some)],
    )
        .prop_map(
            |(input_content, source, speed_mode, podcast_mode, path_suffix, duration_seconds)| {
                SavePodcastInput {
                    input_content,
                    source,
                    speed_mode,
                    podcast_mode,
                    audio_file_path: format!("/tmp/audio/{path_suffix}.mp3"),
                    duration_seconds,
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Saving a query record should persist it with all required fields intact.
    #[test]
    fn query_record_persists_with_all_fields(input in arb_query_input()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        let record = save_query(&db, &input).unwrap();

        // Required fields must be present and match input
        prop_assert_eq!(&record.input_text, &input.input_text);
        prop_assert_eq!(&record.source, &input.source);
        prop_assert_eq!(&record.detected_language, &input.detected_language);
        prop_assert_eq!(&record.analysis_result, &input.analysis_result);
        // ID and timestamp must be non-empty
        prop_assert!(!record.id.is_empty(), "record id must not be empty");
        prop_assert!(!record.created_at.is_empty(), "created_at must not be empty");
    }

    /// Saving a podcast record should persist it with all required fields intact.
    #[test]
    fn podcast_record_persists_with_all_fields(input in arb_podcast_input()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        let record = save_podcast(&db, &input).unwrap();

        // Required fields must be present and match input
        prop_assert_eq!(&record.input_content, &input.input_content);
        prop_assert_eq!(&record.source, &input.source);
        prop_assert_eq!(&record.speed_mode, &input.speed_mode);
        prop_assert_eq!(&record.podcast_mode, &input.podcast_mode);
        prop_assert_eq!(&record.audio_file_path, &input.audio_file_path);
        prop_assert_eq!(record.duration_seconds, input.duration_seconds);
        // ID and timestamp must be non-empty
        prop_assert!(!record.id.is_empty(), "record id must not be empty");
        prop_assert!(!record.created_at.is_empty(), "created_at must not be empty");
    }

    /// Saved query records should be retrievable from history.
    #[test]
    fn query_record_retrievable_from_history(input in arb_query_input()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        let saved = save_query(&db, &input).unwrap();
        let history = db.get_query_records(1, 100).unwrap();

        let found = history.iter().find(|r| r.id == saved.id);
        prop_assert!(found.is_some(), "saved record must appear in query history");
        let found = found.unwrap();
        prop_assert_eq!(&found.input_text, &input.input_text);
        prop_assert_eq!(&found.analysis_result, &input.analysis_result);
    }

    /// Saved podcast records should be retrievable from history.
    #[test]
    fn podcast_record_retrievable_from_history(input in arb_podcast_input()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        let saved = save_podcast(&db, &input).unwrap();
        let history = db.get_podcast_records(1, 100).unwrap();

        let found = history.iter().find(|r| r.id == saved.id);
        prop_assert!(found.is_some(), "saved record must appear in podcast history");
        let found = found.unwrap();
        prop_assert_eq!(&found.input_content, &input.input_content);
        prop_assert_eq!(&found.audio_file_path, &input.audio_file_path);
    }
}
