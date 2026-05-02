//! Audio capture pipeline.
//!
//! Two `pw-record` subprocesses (mic + monitor of the default sink), each
//! producing 48 kHz mono PCM s16. Samples are interleaved to stereo and tee'd
//! into:
//!   - a `tokio::sync::broadcast` channel that the whisper-stream consumer
//!     reads (live transcript preview, U3),
//!   - a tmpfs WAV file that the batch finalize pass and pyannote consume,
//!     and that `--keep-audio` optionally preserves at finalize.
//!
//! `--keep-audio` is *not* consulted here. U2 always writes the WAV; U10
//! decides preserve-vs-unlink at finalize.

pub mod capture;
pub mod interleave;
pub mod source_resolve;
pub mod wav_writer;

pub use capture::{CaptureConfig, CaptureHandle, CaptureQuality, start_capture};
pub use source_resolve::{SinkClass, default_monitor_source, default_sink_class};
