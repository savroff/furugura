//! U3 integration smoke tests.
//!
//! Pure-logic tests live next to their modules (parse_whisper_json,
//! parse_rttm, merge_speaker_labels, parse_stream_line). This file
//! exercises the cross-module wiring with an end-to-end fixture:
//! whisper batch JSON + pyannote RTTM → merged `transcript.jsonl`.
//!
//! The v1 ship gate (≥80% diarization accuracy on a 4-speaker fixture
//! call from the plan's success criterion) requires real whisper.cpp +
//! pyannote and a hand-labeled fixture call; that test is left as
//! `#[ignore]` until the fixture lands.

use furugura::transcribe::{
    LiveSegment, RttmSegment, merge_speaker_labels, read_segments, write_segments,
};
use furugura::transcribe::whisper_batch::parse_whisper_json;
use tempfile::tempdir;

#[test]
fn end_to_end_merge_writes_speaker_labels_to_jsonl() {
    let whisper_json = r#"{
        "transcription": [
          {"text": " Hi there.",   "offsets": {"from": 0,    "to": 1500}},
          {"text": " How are you?","offsets": {"from": 1600, "to": 3000}},
          {"text": " I'm well.",   "offsets": {"from": 3100, "to": 4200}}
        ]
    }"#;
    let batch = parse_whisper_json(whisper_json).unwrap();

    let diarization = vec![
        RttmSegment { start: 0.0, duration: 1.6, speaker: "SPEAKER_00".into() },
        RttmSegment { start: 1.6, duration: 1.5, speaker: "SPEAKER_01".into() },
        RttmSegment { start: 3.1, duration: 1.2, speaker: "SPEAKER_00".into() },
    ];

    let merged = merge_speaker_labels(&batch.segments, &diarization);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].speaker.as_deref(), Some("SPEAKER_00"));
    assert_eq!(merged[1].speaker.as_deref(), Some("SPEAKER_01"));
    assert_eq!(merged[2].speaker.as_deref(), Some("SPEAKER_00"));

    let dir = tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    write_segments(&path, &merged).unwrap();

    let read = read_segments(&path).unwrap();
    assert_eq!(read, merged);
    // Confirm the on-disk JSONL has speaker fields, not nulls.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("SPEAKER_00"));
    assert!(raw.contains("SPEAKER_01"));
}

#[test]
fn unmatched_segments_keep_speaker_none() {
    let segs = vec![LiveSegment {
        t: "00:00:10.000".into(),
        speaker: None,
        text: "alone".into(),
        start: 10.0,
        end: 11.0,
    }];
    let diarization = vec![RttmSegment {
        start: 0.0,
        duration: 1.0,
        speaker: "SPEAKER_00".into(),
    }];
    let merged = merge_speaker_labels(&segs, &diarization);
    assert_eq!(merged[0].speaker, None);
}

#[ignore = "v1 ship gate: requires real whisper.cpp + pyannote + 4-speaker fixture"]
#[test]
fn diarization_accuracy_gate_4speaker() {
    // TODO(v1-ship-gate): hand-labeled 4-speaker fixture call →
    // assert ≥80% segments match the ground-truth speaker label.
    // Held back from CI because it needs:
    //   - real whisper.cpp + GGUF model
    //   - real pyannote install + HF token
    //   - the fixture WAV (not committed yet)
    unreachable!("fixture not yet checked in");
}
