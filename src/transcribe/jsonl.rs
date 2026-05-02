//! JSONL line shape consumed by:
//! - the streaming preview (`transcript.live.jsonl`, written by `whisper_stream`,
//!   tailed by `furu live`),
//! - the authoritative output (`transcript.jsonl`, written at finalize after
//!   merging the whisper batch pass with the pyannote RTTM).
//!
//! One JSON object per line.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// One transcript segment. `speaker` is `None` during the streaming pass and
/// becomes a `SPEAKER_NN` label after the merge step at finalize.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveSegment {
    /// Display timestamp `HH:MM:SS.mmm` relative to meeting start.
    pub t: String,
    /// Speaker label, e.g. `SPEAKER_00`. `None` until diarization merges in.
    #[serde(default)]
    pub speaker: Option<String>,
    pub text: String,
    /// Start time in seconds, relative to meeting start.
    pub start: f64,
    /// End time in seconds, relative to meeting start.
    pub end: f64,
}

/// Render `start_seconds` as `HH:MM:SS.mmm`.
pub fn format_timestamp(start_seconds: f64) -> String {
    let total_ms = (start_seconds * 1000.0).round() as i64;
    let (sign, total_ms) = if total_ms < 0 {
        ("-", -total_ms)
    } else {
        ("", total_ms)
    };
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{sign}{h:02}:{m:02}:{s:02}.{ms:03}")
}

/// Append a segment to a JSONL file (one segment per line).
/// Used by the streaming pass during the meeting.
pub fn append_live_segment(path: &Path, segment: &LiveSegment) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("could not open JSONL for append: {}", path.display()))?;
    let line = serde_json::to_string(segment)
        .context("could not serialize transcript segment")?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Write a full set of segments, one per line, atomically truncating any
/// previous content. Used for the authoritative `transcript.jsonl` at finalize.
pub fn write_segments(path: &Path, segments: &[LiveSegment]) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("could not open JSONL for write: {}", path.display()))?;
    for s in segments {
        let line = serde_json::to_string(s)?;
        writeln!(f, "{line}")?;
    }
    f.flush()?;
    Ok(())
}

/// Read all segments from a JSONL file. Skips blank lines; tolerates a
/// `meta` line (`{"meta":"..."}`) by ignoring it.
pub fn read_segments(path: &Path) -> Result<Vec<LiveSegment>> {
    let f = std::fs::File::open(path)
        .with_context(|| format!("could not open JSONL for read: {}", path.display()))?;
    let r = BufReader::new(f);
    let mut out = Vec::new();
    for (idx, line) in r.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Tolerate meta lines.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
            && v.get("meta").is_some()
        {
            continue;
        }
        let seg: LiveSegment = serde_json::from_str(line).with_context(|| {
            format!("invalid JSONL at {}:{}", path.display(), idx + 1)
        })?;
        out.push(seg);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn timestamp_format_zero() {
        assert_eq!(format_timestamp(0.0), "00:00:00.000");
    }

    #[test]
    fn timestamp_format_seconds_only() {
        assert_eq!(format_timestamp(7.500), "00:00:07.500");
    }

    #[test]
    fn timestamp_format_over_minute() {
        assert_eq!(format_timestamp(83.250), "00:01:23.250");
    }

    #[test]
    fn timestamp_format_over_hour() {
        assert_eq!(format_timestamp(3661.001), "01:01:01.001");
    }

    #[test]
    fn append_then_read_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("transcript.live.jsonl");
        let s1 = LiveSegment {
            t: "00:00:01.000".into(),
            speaker: None,
            text: "Hello.".into(),
            start: 1.0,
            end: 1.4,
        };
        let s2 = LiveSegment {
            t: "00:00:02.000".into(),
            speaker: None,
            text: "World.".into(),
            start: 2.0,
            end: 2.3,
        };
        append_live_segment(&path, &s1).unwrap();
        append_live_segment(&path, &s2).unwrap();
        let read = read_segments(&path).unwrap();
        assert_eq!(read, vec![s1, s2]);
    }

    #[test]
    fn read_skips_meta_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let s = LiveSegment {
            t: "00:00:00.000".into(),
            speaker: None,
            text: "ok".into(),
            start: 0.0,
            end: 0.5,
        };
        std::fs::write(
            &path,
            format!(
                "{{\"meta\":\"diarization_skipped\",\"reason\":\"hf_token_missing\"}}\n{}\n",
                serde_json::to_string(&s).unwrap(),
            ),
        )
        .unwrap();
        let read = read_segments(&path).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0], s);
    }

    #[test]
    fn read_blank_lines_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let s = LiveSegment {
            t: "00:00:00.000".into(),
            speaker: None,
            text: "ok".into(),
            start: 0.0,
            end: 0.5,
        };
        std::fs::write(
            &path,
            format!("\n\n{}\n\n", serde_json::to_string(&s).unwrap()),
        )
        .unwrap();
        let read = read_segments(&path).unwrap();
        assert_eq!(read.len(), 1);
    }

    #[test]
    fn write_segments_truncates() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "garbage\nmore garbage\n").unwrap();
        let s = LiveSegment {
            t: "00:00:00.000".into(),
            speaker: Some("SPEAKER_00".into()),
            text: "ok".into(),
            start: 0.0,
            end: 0.5,
        };
        write_segments(&path, std::slice::from_ref(&s)).unwrap();
        let read = read_segments(&path).unwrap();
        assert_eq!(read, vec![s]);
    }

    #[test]
    fn invalid_json_in_jsonl_returns_error_naming_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "not-json\n").unwrap();
        let err = read_segments(&path).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains(":1"), "error should name line 1: {msg}");
    }
}
