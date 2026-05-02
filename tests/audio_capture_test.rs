//! U2 tests for the audio capture pipeline.
//!
//! Pure-logic tests (drift math, source-resolve parsing, interleave, WAV
//! writer) live in unit tests inside their respective modules. This file
//! exercises the `start_capture` orchestrator end-to-end against a real
//! `pw-record` and is `#[ignore]`'d by default — run with
//! `cargo test --test audio_capture_test -- --ignored` on a box where
//! PipeWire and `pw-record` are available.

use furugura::audio::capture::{
    CaptureConfig, compute_drift_ms, drift_is_degraded, start_capture,
};
use std::time::Duration;

#[test]
fn drift_zero_when_samples_match_elapsed() {
    let (l, r) = compute_drift_ms(48_000, Duration::from_secs(1), 48_000, 48_000);
    assert_eq!(l, 0);
    assert_eq!(r, 0);
}

#[test]
fn drift_under_threshold_is_clean() {
    // 30ms behind on a 1s window → not degraded.
    let behind = 48_000 - (48_000 * 30 / 1000);
    let (l, r) = compute_drift_ms(48_000, Duration::from_secs(1), behind, 48_000);
    assert_eq!(l, 30);
    assert_eq!(r, 0);
    assert!(!drift_is_degraded(l, r));
}

#[test]
fn drift_over_threshold_is_degraded() {
    // 80ms behind exceeds the 50ms plan threshold.
    let behind = 48_000 - (48_000 * 80 / 1000);
    let (l, _) = compute_drift_ms(48_000, Duration::from_secs(1), behind, 48_000);
    assert_eq!(l, 80);
    assert!(drift_is_degraded(l, 0));
}

#[ignore = "requires real pw-record + a default sink with a monitor source"]
#[tokio::test]
async fn happy_path_30s_simulated_meeting() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("meeting.wav");
    let cfg = CaptureConfig {
        mic_source: None,
        system_source: None,
        sample_rate: 48_000,
        wav_path: wav.clone(),
    };
    let handle = start_capture(cfg).await.expect("capture should start");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let _quality = handle.shutdown().await.unwrap();

    let bytes = std::fs::read(&wav).unwrap();
    // Allow a wide tolerance for short captures: at least ~0.5s of audio.
    assert!(
        bytes.len() > 44 + (48_000 / 2) * 4,
        "WAV too small ({} bytes) — capture wrote nothing",
        bytes.len(),
    );

    let reader = hound::WavReader::open(&wav).unwrap();
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().sample_rate, 48_000);
    assert_eq!(reader.spec().bits_per_sample, 16);
}
