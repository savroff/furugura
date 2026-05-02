//! Transcription pipeline: whisper.cpp (streaming + batch), pyannote
//! diarization, and the segment+speaker merge that produces the
//! authoritative `transcript.jsonl` for U10.
//!
//! The streaming pass runs during the meeting (consuming the broadcast
//! channel from U2) writing low-latency segments to `transcript.live.jsonl`.
//! The batch pass runs at finalize for higher accuracy; pyannote runs
//! once on the full WAV; the merge aligns whisper segments to RTTM
//! diarization segments by timestamp overlap.

pub mod jsonl;
pub mod merge;
pub mod pyannote;
pub mod rttm;
pub mod whisper_batch;
pub mod whisper_stream;

pub use jsonl::{LiveSegment, append_live_segment, read_segments, write_segments};
pub use merge::merge_speaker_labels;
pub use rttm::{RttmSegment, parse_rttm, write_rttm};
pub use whisper_batch::WhisperBatchOutput;
