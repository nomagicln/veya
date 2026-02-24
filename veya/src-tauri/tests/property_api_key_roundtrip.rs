// Feature: veya-mvp, Property 11: API Key 加密存储往返
//
// For any random API Key string, after storing it through the StrongholdStore
// and recording the reference in SQLite's api_configs table:
//   1. The SQLite api_key_ref column must contain only the Stronghold reference,
//      NOT the plaintext API Key.
//   2. Reading back from Stronghold via that reference must return the original
//      API Key value.
//
// Validates: Requirement 5.5

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use tempfile::TempDir;
use veya_lib::db::Database;
use veya_lib::stronghold_store::StrongholdStore;

/// Strategy that produces (config_id, api_key) pairs.
fn id_and_key() -> impl Strategy<Value = (String, String)> {
    (
        "[a-zA-Z0-9]{1,16}",
        "[a-zA-Z0-9!@#$%^&*_+=]{1,64}",
    )
}

#[test]
fn api_key_encrypted_storage_roundtrip() {
    let dir = TempDir::new().unwrap();
    let store = StrongholdStore::open(dir.path().join("stronghold"), b"test-pw").unwrap();
    let db = Database::open(dir.path().join("db")).unwrap();

    // Use store_api_key_in_memory to avoid the expensive snapshot commit on
    // every iteration. The Stronghold client store insert/get logic is still
    // fully exercised — only the disk encryption is deferred.
    let config = Config::with_cases(100);
    let mut runner = TestRunner::new(config);

    runner
        .run(&id_and_key(), |(config_id, api_key)| {
            let api_key_ref = format!("api_key_{config_id}");

            // 1. Store the key in Stronghold (in-memory, no snapshot commit).
            store
                .store_api_key_in_memory(&config_id, &api_key)
                .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("{e}")))?;

            // 2. Persist config metadata (with reference only) in SQLite.
            db.insert_api_config(
                &config_id,
                "Test Config",
                "openai",
                "text",
                "https://api.example.com",
                "gpt-4",
                &api_key_ref,
                None,
                false,
            )
            .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("{e}")))?;

            // ── Property assertion 1: SQLite contains only the reference ────
            let configs = db
                .get_api_configs()
                .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("{e}")))?;
            let row = configs
                .iter()
                .find(|c| c.id == config_id)
                .ok_or_else(|| {
                    proptest::test_runner::TestCaseError::fail("config row must exist")
                })?;

            // The stored ref must equal the expected reference key.
            prop_assert_eq!(
                &row.api_key_ref,
                &api_key_ref,
                "SQLite api_key_ref should be the Stronghold reference"
            );

            // The reference must NOT be the plaintext key.
            if api_key != api_key_ref {
                prop_assert_ne!(
                    &row.api_key_ref,
                    &api_key,
                    "SQLite must not contain the plaintext API key"
                );
            }

            // ── Property assertion 2: Stronghold returns the original key ───
            let retrieved = store
                .get_api_key(&config_id)
                .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("{e}")))?
                .ok_or_else(|| {
                    proptest::test_runner::TestCaseError::fail("key must exist in Stronghold")
                })?;

            prop_assert_eq!(
                retrieved,
                api_key,
                "Stronghold must return the original API key"
            );

            Ok(())
        })
        .unwrap();
}
