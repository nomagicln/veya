// Feature: veya-mvp, Property 14: 缓存清理策略
//
// For any cache cleanup configuration (max space M, max days D), after cleanup
// the directory size should be ≤ M and no files older than D days should remain.
//
// Validates: Requirements 7.3

use proptest::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

use veya_lib::cast_engine::cleanup_by_policy;

/// Helper: write a file with specific content and set its modified time to `age`
/// seconds in the past.
fn write_file_with_age(dir: &PathBuf, name: &str, data: &[u8], age_secs: u64) -> PathBuf {
    fs::create_dir_all(dir).expect("create dir");
    let path = dir.join(name);
    fs::write(&path, data).expect("write file");

    // Set modified time to `age_secs` in the past.
    let mtime = filetime::FileTime::from_system_time(
        SystemTime::now() - Duration::from_secs(age_secs),
    );
    filetime::set_file_mtime(&path, mtime).expect("set mtime");
    path
}

/// Compute total size of all files in a directory.
fn dir_total_size(dir: &PathBuf) -> u64 {
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Collect modified times of remaining files.
fn remaining_ages(dir: &PathBuf) -> Vec<Duration> {
    let now = SystemTime::now();
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified = meta.modified().ok()?;
            now.duration_since(modified).ok()
        })
        .collect()
}

// ── Strategies ───────────────────────────────────────────────────

/// Max size in MB: 1..50 (small values to make budget constraints trigger).
fn max_size_mb_strategy() -> impl Strategy<Value = u64> {
    1u64..50
}

/// Max days: 1..90.
fn max_days_strategy() -> impl Strategy<Value = u32> {
    1u32..90
}

/// File age in seconds. We generate ages spanning from fresh (0) to very old
/// (180 days) so some files will exceed the max_days threshold.
fn age_secs_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        0u64..3600,                    // fresh: < 1 hour
        86_400u64..86_400 * 30,        // moderate: 1-30 days
        86_400u64 * 60..86_400 * 180,  // old: 60-180 days
    ]
}

/// File data: sized between 100 bytes and 500KB to exercise budget trimming.
fn file_data_strategy() -> impl Strategy<Value = Vec<u8>> {
    (100usize..500_000).prop_flat_map(|size| {
        proptest::collection::vec(any::<u8>(), size..=size)
    })
}

fn filename_strategy() -> impl Strategy<Value = String> {
    "[a-f0-9]{8}\\.mp3"
}

/// A collection of files with ages and data.
fn files_strategy() -> impl Strategy<Value = Vec<(String, Vec<u8>, u64)>> {
    proptest::collection::vec(
        (filename_strategy(), file_data_strategy(), age_secs_strategy()),
        1..10,
    )
}

// ── Property tests ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// After cleanup, no file should be older than max_days.
    #[test]
    fn no_files_exceed_max_age(
        files in files_strategy(),
        max_days in max_days_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("saved");

        for (name, data, age) in &files {
            write_file_with_age(&dir, name, data, *age);
        }

        // Use a very large budget so only age-based removal triggers.
        cleanup_by_policy(&dir, 999_999, max_days).unwrap();

        let max_age = Duration::from_secs(max_days as u64 * 86_400);
        for age in remaining_ages(&dir) {
            prop_assert!(
                age <= max_age + Duration::from_secs(2), // small tolerance for test execution time
                "file age {:?} exceeds max {:?}",
                age,
                max_age
            );
        }
    }

    /// After cleanup, total directory size should be ≤ max_size_mb.
    #[test]
    fn total_size_within_budget(
        files in files_strategy(),
        max_size_mb in max_size_mb_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("saved");

        for (name, data, _age) in &files {
            // All files are fresh so age-based removal won't interfere.
            write_file_with_age(&dir, name, data, 0);
        }

        cleanup_by_policy(&dir, max_size_mb, 9999).unwrap();

        let max_bytes = max_size_mb * 1_024 * 1_024;
        let actual = dir_total_size(&dir);
        prop_assert!(
            actual <= max_bytes,
            "dir size {} exceeds budget {} bytes",
            actual,
            max_bytes
        );
    }

    /// Combined: both age and size constraints are satisfied simultaneously.
    #[test]
    fn combined_age_and_size_constraints(
        files in files_strategy(),
        max_size_mb in max_size_mb_strategy(),
        max_days in max_days_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("saved");

        for (name, data, age) in &files {
            write_file_with_age(&dir, name, data, *age);
        }

        cleanup_by_policy(&dir, max_size_mb, max_days).unwrap();

        // Check size constraint.
        let max_bytes = max_size_mb * 1_024 * 1_024;
        let actual_size = dir_total_size(&dir);
        prop_assert!(
            actual_size <= max_bytes,
            "dir size {} exceeds budget {} bytes",
            actual_size,
            max_bytes
        );

        // Check age constraint.
        let max_age = Duration::from_secs(max_days as u64 * 86_400);
        for age in remaining_ages(&dir) {
            prop_assert!(
                age <= max_age + Duration::from_secs(2),
                "file age {:?} exceeds max {:?}",
                age,
                max_age
            );
        }
    }

    /// Cleanup on an empty directory is a no-op (no panic, no error).
    #[test]
    fn cleanup_empty_dir_is_noop(
        max_size_mb in max_size_mb_strategy(),
        max_days in max_days_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("saved");
        fs::create_dir_all(&dir).unwrap();

        let result = cleanup_by_policy(&dir, max_size_mb, max_days);
        prop_assert!(result.is_ok());
        prop_assert_eq!(dir_total_size(&dir), 0);
    }

    /// Files within both age and size limits should survive cleanup.
    #[test]
    fn fresh_small_files_survive(
        data in proptest::collection::vec(any::<u8>(), 100..1000),
        max_days in max_days_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("saved");

        // Write a single small, fresh file.
        let path = write_file_with_age(&dir, "fresh.mp3", &data, 0);

        // Budget is generous: 100 MB, and file is fresh.
        cleanup_by_policy(&dir, 100, max_days).unwrap();

        prop_assert!(path.exists(), "fresh small file should survive cleanup");
        let content = fs::read(&path).unwrap();
        prop_assert_eq!(&content, &data, "file content should be unchanged");
    }
}
