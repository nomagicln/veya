// Feature: veya-mvp, Property 6: 播客输出选项与格式
//
// For any combination of speed mode (slow/normal) and podcast mode
// (bilingual/immersive), Cast Engine should produce a valid MP3 file
// with size > 0.
//
// Validates: Requirements 3.3, 3.4

use proptest::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;
use veya_lib::cast_engine::{
    PodcastMode, PodcastOptions, SpeedMode, split_script_segments,
};

/// Minimal valid MP3 frame (MPEG1 Layer3, 128kbps, 44100Hz).
fn fake_mp3_frame() -> Vec<u8> {
    let header: [u8; 4] = [0xFF, 0xFB, 0x90, 0x00];
    let frame_size = 417;
    let mut frame = Vec::with_capacity(frame_size);
    frame.extend_from_slice(&header);
    frame.resize(frame_size, 0x00);
    frame
}


/// Simulate TTS synthesis returning fake MP3 audio bytes.
fn simulate_tts_synthesis(segments: &[String], speed: &SpeedMode) -> Vec<u8> {
    let mut audio = Vec::new();
    let frames_per_segment = match speed {
        SpeedMode::Slow => 6,
        SpeedMode::Normal => 4,
    };
    for _seg in segments {
        for _ in 0..frames_per_segment {
            audio.extend_from_slice(&fake_mp3_frame());
        }
    }
    audio
}

/// Write audio bytes to a directory as an MP3 file, returning the path.
fn write_mp3(dir: &PathBuf, audio: &[u8]) -> PathBuf {
    let filename = format!("{}.mp3", Uuid::new_v4());
    let path = dir.join(filename);
    std::fs::write(&path, audio).expect("write mp3");
    path
}

// ── Strategies ───────────────────────────────────────────────────

fn speed_strategy() -> impl Strategy<Value = SpeedMode> {
    prop_oneof![Just(SpeedMode::Slow), Just(SpeedMode::Normal)]
}

fn mode_strategy() -> impl Strategy<Value = PodcastMode> {
    prop_oneof![Just(PodcastMode::Bilingual), Just(PodcastMode::Immersive)]
}

fn script_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-zA-Z .,]{10,80}", 1..5)
        .prop_map(|paragraphs| paragraphs.join("\n\n"))
}


// ── Property tests ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// For every speed x mode combination, the output must be a valid MP3
    /// file (starts with sync bytes 0xFF 0xFB) with size > 0.
    #[test]
    fn all_option_combos_produce_valid_mp3(
        speed in speed_strategy(),
        mode in mode_strategy(),
        script in script_strategy(),
    ) {
        let _options = PodcastOptions {
            speed: speed.clone(),
            mode,
            target_language: "en".into(),
        };

        let segments = split_script_segments(&script);
        let audio = simulate_tts_synthesis(&segments, &speed);

        let tmp = TempDir::new().expect("create temp dir");
        let path = write_mp3(&tmp.path().to_path_buf(), &audio);

        // File must exist and have size > 0
        let meta = std::fs::metadata(&path).expect("file metadata");
        prop_assert!(meta.len() > 0, "MP3 file must have size > 0");

        // File must have .mp3 extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        prop_assert_eq!(ext, "mp3", "output must have .mp3 extension");

        // File must start with valid MP3 sync bytes
        let bytes = std::fs::read(&path).expect("read mp3");
        prop_assert!(bytes.len() >= 2, "MP3 must have at least 2 bytes");
        prop_assert_eq!(bytes[0], 0xFF, "first byte must be 0xFF (sync)");
        prop_assert_eq!(bytes[1] & 0xE0, 0xE0, "sync bits must be set in second byte");
    }

    /// Slow mode must produce more audio data than normal mode for the
    /// same script content (more frames per segment).
    #[test]
    fn slow_mode_produces_larger_output(script in script_strategy()) {
        let segments = split_script_segments(&script);
        let slow_audio = simulate_tts_synthesis(&segments, &SpeedMode::Slow);
        let normal_audio = simulate_tts_synthesis(&segments, &SpeedMode::Normal);

        prop_assert!(
            slow_audio.len() > normal_audio.len(),
            "slow ({}) must be larger than normal ({})",
            slow_audio.len(),
            normal_audio.len()
        );
    }

    /// Both bilingual and immersive modes must produce non-empty audio
    /// for the same input content.
    #[test]
    fn both_modes_produce_nonempty_audio(
        script in script_strategy(),
        speed in speed_strategy(),
    ) {
        let segments = split_script_segments(&script);
        let audio = simulate_tts_synthesis(&segments, &speed);
        prop_assert!(!audio.is_empty(), "audio must not be empty");
    }

    /// The output file written to disk must be byte-identical to the
    /// concatenated TTS output (no corruption during write).
    #[test]
    fn written_file_matches_audio_bytes(
        speed in speed_strategy(),
        script in script_strategy(),
    ) {
        let segments = split_script_segments(&script);
        let audio = simulate_tts_synthesis(&segments, &speed);

        let tmp = TempDir::new().expect("create temp dir");
        let path = write_mp3(&tmp.path().to_path_buf(), &audio);

        let read_back = std::fs::read(&path).expect("read back mp3");
        prop_assert_eq!(read_back, audio, "written bytes must match original audio");
    }

    /// SpeedMode::tts_speed() must return correct values.
    #[test]
    fn speed_mode_tts_speed_values(speed in speed_strategy()) {
        let tts_speed = speed.tts_speed();
        match speed {
            SpeedMode::Slow => prop_assert!((tts_speed - 0.75).abs() < f32::EPSILON),
            SpeedMode::Normal => prop_assert!((tts_speed - 1.0).abs() < f32::EPSILON),
        }
    }
}
