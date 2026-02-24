// Feature: veya-mvp, Property 13: 词频统计准确性
//
// For any sequence of queries, the frequent words list should report a count
// for each word that equals the total number of times that word appeared
// across all query inputs.
//
// Validates: Requirements 6.3

use std::collections::HashMap;

use proptest::prelude::*;
use tempfile::TempDir;
use veya_lib::db::Database;
use veya_lib::learning_record::{save_query, tokenize, SaveQueryInput};

/// Strategy for generating a non-empty text string suitable for tokenisation.
fn arb_text() -> impl Strategy<Value = String> {
    "[a-zA-Z]{1,8}( [a-zA-Z]{1,8}){0,5}"
}

/// Strategy for generating a small sequence of query inputs (1–5 queries).
fn arb_query_sequence() -> impl Strategy<Value = Vec<SaveQueryInput>> {
    prop::collection::vec(arb_text(), 1..=5).prop_map(|texts| {
        texts
            .into_iter()
            .map(|text| SaveQueryInput {
                input_text: text,
                source: "text_insight".to_string(),
                detected_language: Some("en".to_string()),
                analysis_result: "{}".to_string(),
            })
            .collect()
    })
}

/// Build the expected word→count map by tokenising every query input.
fn expected_frequencies(queries: &[SaveQueryInput]) -> HashMap<String, i64> {
    let mut freq: HashMap<String, i64> = HashMap::new();
    for q in queries {
        for word in tokenize(&q.input_text) {
            *freq.entry(word).or_insert(0) += 1;
        }
    }
    freq
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// For any sequence of queries, every word's stored frequency must equal
    /// the total occurrences of that word across all query inputs.
    #[test]
    fn word_frequency_matches_total_occurrences(queries in arb_query_sequence()) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        // Save all queries
        for q in &queries {
            save_query(&db, q).unwrap();
        }

        let expected = expected_frequencies(&queries);

        // Retrieve stored frequencies (use a large limit to get all words)
        let stored = db.get_frequent_words(1000).unwrap();
        let stored_map: HashMap<String, i64> = stored
            .into_iter()
            .map(|row| (row.word, row.count))
            .collect();

        // Every expected word must be present with the correct count
        for (word, expected_count) in &expected {
            let actual = stored_map.get(word).copied().unwrap_or(0);
            prop_assert_eq!(
                actual,
                *expected_count,
                "word '{}': expected count {}, got {}",
                word,
                expected_count,
                actual
            );
        }

        // No extra words should exist beyond what we expect
        for (word, count) in &stored_map {
            prop_assert!(
                expected.contains_key(word),
                "unexpected word '{}' with count {} in frequency table",
                word,
                count
            );
        }
    }
}
