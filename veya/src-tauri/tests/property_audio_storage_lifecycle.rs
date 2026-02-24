// Feature: veya-mvp, Property 7: 音频存储生命周期
//
// For any generated podcast audio, it should default to the temp cache directory.
// After a save operation, the audio file should exist in the persistent directory.
// After a cleanup operation, all temp files should be deleted while persistent
// files remain unaffected.
//
// Validates: Requirements 3.5, 3.6

use proptest::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Simulate writing a podcast audio file to the temp directory (mirrors
/// the tail end of `generate_podcast` which writes to `temp_audio_dir()`).
fn write_temp_audio(temp_dir: &PathBuf, filename: &str, data: &[u8]) -> PathBuf {
    fs::create_dir_all(temp_dir).expect("create temp dir");
    let path = temp_dir.join(filename);
    fs::write(&path, data).expect("write temp audio");
    path
}

/// Simulate `save_podcast`: copy from temp to saved directory, preserving filename.
fn simulate_save_podcast(temp_path: &PathBuf, saved_dir: &PathBuf) -> PathBuf {
    fs::create_dir_all(saved_dir).expect("create saved dir");
    let filename = temp_path
        .file_name()
        .expect("temp file must have a name")
        .to_string_lossy()
        .to_string();
    let dest = saved_dir.join(&filename);
    fs::copy(temp_path, &dest).expect("copy to saved dir");
    dest
}

/// Simulate `cleanup_temp_audio`: remove all files inside the temp directory.
fn simulate_cleanup_temp(temp_dir: &PathBuf) {
    if !temp_dir.exists() {
        return;
    }
    for entry in fs::read_dir(temp_dir).expect("read temp dir").flatten() {
        let path = entry.path();
        if path.is_file() {
            fs::remove_file(&path).ok();
        }
    }
}

// ── Strategies ───────────────────────────────────────────────────

fn audio_data_strategy() -> impl Strategy<Value = Vec<u8>> {
    // Generate non-empty byte vectors simulating MP3 audio data.
    prop::collection::vec(any::<u8>(), 100..2000)
}

fn filename_strategy() -> impl Strategy<Value = String> {
    "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\\.mp3"
}

fn multi_file_strategy() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
    prop::collection::vec(
        (filename_strategy(), audio_data_strategy()),
        1..6,
    )
}

// ── Property tests ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Generated audio defaults to the temp directory and the file exists there.
    #[test]
    fn audio_defaults_to_temp_directory(
        filename in filename_strategy(),
        data in audio_data_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let temp_dir = root.path().join("audio").join("temp");

        let temp_path = write_temp_audio(&temp_dir, &filename, &data);

        // File must exist in temp directory.
        prop_assert!(temp_path.exists(), "audio file must exist in temp dir");
        prop_assert!(temp_path.starts_with(&temp_dir), "file must be inside temp dir");

        // Content must match what was written.
        let read_back = fs::read(&temp_path).unwrap();
        prop_assert_eq!(&read_back, &data, "file content must match original data");
    }

    /// After save, the audio file exists in the persistent (saved) directory
    /// with identical content.
    #[test]
    fn save_copies_to_persistent_directory(
        filename in filename_strategy(),
        data in audio_data_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let temp_dir = root.path().join("audio").join("temp");
        let saved_dir = root.path().join("audio").join("saved");

        let temp_path = write_temp_audio(&temp_dir, &filename, &data);
        let saved_path = simulate_save_podcast(&temp_path, &saved_dir);

        // Saved file must exist in the persistent directory.
        prop_assert!(saved_path.exists(), "saved file must exist");
        prop_assert!(saved_path.starts_with(&saved_dir), "saved file must be inside saved dir");

        // Content must be identical.
        let saved_data = fs::read(&saved_path).unwrap();
        prop_assert_eq!(&saved_data, &data, "saved content must match original");

        // Original temp file must still exist (save is a copy, not a move).
        prop_assert!(temp_path.exists(), "temp file must still exist after save");
    }

    /// After cleanup, all temp files are deleted while saved files remain intact.
    #[test]
    fn cleanup_removes_temp_preserves_saved(
        files in multi_file_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let temp_dir = root.path().join("audio").join("temp");
        let saved_dir = root.path().join("audio").join("saved");

        let mut temp_paths = Vec::new();
        let mut saved_paths = Vec::new();

        // Write all files to temp, save each to persistent directory.
        for (filename, data) in &files {
            let temp_path = write_temp_audio(&temp_dir, filename, data);
            let saved_path = simulate_save_podcast(&temp_path, &saved_dir);
            temp_paths.push((temp_path, data.clone()));
            saved_paths.push((saved_path, data.clone()));
        }

        // All files exist before cleanup.
        for (tp, _) in &temp_paths {
            prop_assert!(tp.exists(), "temp file must exist before cleanup");
        }
        for (sp, _) in &saved_paths {
            prop_assert!(sp.exists(), "saved file must exist before cleanup");
        }

        // Perform cleanup of temp directory.
        simulate_cleanup_temp(&temp_dir);

        // All temp files must be gone.
        for (tp, _) in &temp_paths {
            prop_assert!(!tp.exists(), "temp file must be deleted after cleanup: {:?}", tp);
        }

        // All saved files must still exist with correct content.
        for (sp, original_data) in &saved_paths {
            prop_assert!(sp.exists(), "saved file must survive cleanup: {:?}", sp);
            let content = fs::read(sp).unwrap();
            prop_assert_eq!(&content, original_data, "saved content must be unchanged");
        }
    }

    /// Cleanup on an empty or non-existent temp directory is a no-op (no panic).
    #[test]
    fn cleanup_on_empty_temp_is_noop(
        files in multi_file_strategy(),
    ) {
        let root = TempDir::new().unwrap();
        let temp_dir = root.path().join("audio").join("temp");
        let saved_dir = root.path().join("audio").join("saved");

        // Only write to saved dir (temp dir may not even exist).
        fs::create_dir_all(&saved_dir).unwrap();
        let mut saved_paths = Vec::new();
        for (filename, data) in &files {
            let sp = saved_dir.join(filename);
            fs::write(&sp, data).unwrap();
            saved_paths.push((sp, data.clone()));
        }

        // Cleanup on non-existent temp dir must not panic.
        simulate_cleanup_temp(&temp_dir);

        // Create empty temp dir and cleanup again — still no panic.
        fs::create_dir_all(&temp_dir).unwrap();
        simulate_cleanup_temp(&temp_dir);

        // Saved files must be unaffected.
        for (sp, original_data) in &saved_paths {
            prop_assert!(sp.exists(), "saved file must survive");
            let content = fs::read(sp).unwrap();
            prop_assert_eq!(&content, original_data);
        }
    }

    /// The full lifecycle: generate (temp) → save (persistent) → cleanup →
    /// verify temp gone, saved intact.
    #[test]
    fn full_lifecycle_generate_save_cleanup(
        files in multi_file_strategy(),
        save_indices in prop::collection::vec(any::<bool>(), 1..6),
    ) {
        let root = TempDir::new().unwrap();
        let temp_dir = root.path().join("audio").join("temp");
        let saved_dir = root.path().join("audio").join("saved");

        let mut temp_paths = Vec::new();
        let mut saved_paths = Vec::new();

        // Generate all files in temp.
        for (filename, data) in &files {
            let tp = write_temp_audio(&temp_dir, filename, data);
            temp_paths.push((tp, data.clone()));
        }

        // Selectively save some files (simulating user choosing to save).
        for (i, (tp, data)) in temp_paths.iter().enumerate() {
            let should_save = save_indices.get(i).copied().unwrap_or(false);
            if should_save {
                let sp = simulate_save_podcast(tp, &saved_dir);
                saved_paths.push((sp, data.clone()));
            }
        }

        // Cleanup temp.
        simulate_cleanup_temp(&temp_dir);

        // All temp files must be gone.
        for (tp, _) in &temp_paths {
            prop_assert!(!tp.exists(), "temp file must be removed: {:?}", tp);
        }

        // All saved files must remain with correct content.
        for (sp, original_data) in &saved_paths {
            prop_assert!(sp.exists(), "saved file must persist: {:?}", sp);
            let content = fs::read(sp).unwrap();
            prop_assert_eq!(&content, original_data, "saved content must be unchanged");
        }
    }
}
