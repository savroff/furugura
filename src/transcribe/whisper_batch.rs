//! Batch whisper.cpp pass invoked at finalize.
//!
//! Runs `whisper-cli <wav> --model <gguf> --output-json --output-file <stem>`,
//! reads `<stem>.json`, and parses it into `LiveSegment`s. Word-level
//! timestamps may be absent for digits/symbols (whisper.cpp issue) — we
//! tolerate that at parse time.

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use super::jsonl::{LiveSegment, format_timestamp};

#[derive(Debug, Clone)]
pub struct WhisperBatchOutput {
    pub segments: Vec<LiveSegment>,
}

#[derive(Debug, Clone)]
pub struct WhisperBatchConfig {
    pub model_path: PathBuf,
    pub wav_path: PathBuf,
    /// Output stem; whisper.cpp will write `<stem>.json` next to it.
    pub output_stem: PathBuf,
    pub threads: Option<u32>,
    /// Override the binary name (default: `whisper-cli`).
    pub binary: Option<String>,
}

/// Run the batch whisper.cpp pass and return parsed segments.
pub async fn run(cfg: WhisperBatchConfig) -> Result<WhisperBatchOutput> {
    let bin = cfg.binary.as_deref().unwrap_or("whisper-cli");

    let mut cmd = Command::new(bin);
    cmd.arg(&cfg.wav_path);
    cmd.arg("--model").arg(&cfg.model_path);
    cmd.arg("--output-json");
    cmd.arg("--output-file").arg(&cfg.output_stem);
    if let Some(t) = cfg.threads {
        cmd.arg("--threads").arg(t.to_string());
    }
    let output = cmd
        .output()
        .await
        .with_context(|| format!("could not run `{bin}` (is it on $PATH?)"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{bin} exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    let json_path = with_json_extension(&cfg.output_stem);
    let raw = std::fs::read_to_string(&json_path).with_context(|| {
        format!(
            "{bin} did not produce expected JSON at {}",
            json_path.display(),
        )
    })?;
    parse_whisper_json(&raw)
}

fn with_json_extension(stem: &Path) -> PathBuf {
    let mut p = stem.to_path_buf();
    p.set_extension("json");
    p
}

/// Parse the JSON whisper.cpp writes when invoked with `--output-json`.
/// The shape we depend on is permissive — only the segments array is
/// required, and only `text` + (one of `offsets` or `timestamps`) per
/// segment.
pub fn parse_whisper_json(raw: &str) -> Result<WhisperBatchOutput> {
    let parsed: WhisperJson = serde_json::from_str(raw)
        .context("could not parse whisper.cpp JSON output")?;

    let segments_in = parsed
        .transcription
        .ok_or_else(|| anyhow!("whisper JSON had no `transcription` array"))?;

    let mut segments = Vec::with_capacity(segments_in.len());
    for seg in segments_in {
        let (start_s, end_s) = match (&seg.offsets, &seg.timestamps) {
            (Some(o), _) => (o.from as f64 / 1000.0, o.to as f64 / 1000.0),
            (None, Some(ts)) => (
                parse_whisper_clock(&ts.from)?,
                parse_whisper_clock(&ts.to)?,
            ),
            (None, None) => return Err(anyhow!(
                "whisper segment had neither offsets nor timestamps",
            )),
        };
        let text = seg.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        segments.push(LiveSegment {
            t: format_timestamp(start_s),
            speaker: None,
            text,
            start: start_s,
            end: end_s,
        });
    }
    Ok(WhisperBatchOutput { segments })
}

/// Parse `HH:MM:SS,mmm` or `HH:MM:SS.mmm` to seconds.
fn parse_whisper_clock(s: &str) -> Result<f64> {
    let s = s.replace(',', ".");
    let mut parts = s.split(':');
    let h: f64 = parts
        .next()
        .ok_or_else(|| anyhow!("missing hour"))?
        .parse()
        .context("invalid hour")?;
    let m: f64 = parts
        .next()
        .ok_or_else(|| anyhow!("missing minute"))?
        .parse()
        .context("invalid minute")?;
    let sec: f64 = parts
        .next()
        .ok_or_else(|| anyhow!("missing second"))?
        .parse()
        .context("invalid second")?;
    if parts.next().is_some() {
        return Err(anyhow!("unexpected extra ':' in timestamp: {s}"));
    }
    Ok(h * 3600.0 + m * 60.0 + sec)
}

#[derive(Debug, Deserialize)]
struct WhisperJson {
    transcription: Option<Vec<WhisperSegment>>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    text: String,
    #[serde(default)]
    offsets: Option<WhisperOffsets>,
    #[serde(default)]
    timestamps: Option<WhisperTimestamps>,
}

#[derive(Debug, Deserialize)]
struct WhisperOffsets {
    from: i64,
    to: i64,
}

#[derive(Debug, Deserialize)]
struct WhisperTimestamps {
    from: String,
    to: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offsets_form() {
        let raw = r#"{
            "transcription": [
              {"text": " Hello.", "offsets": {"from": 0, "to": 1500}},
              {"text": " World.", "offsets": {"from": 1500, "to": 3000}}
            ]
        }"#;
        let out = parse_whisper_json(raw).unwrap();
        assert_eq!(out.segments.len(), 2);
        assert_eq!(out.segments[0].text, "Hello.");
        assert_eq!(out.segments[0].start, 0.0);
        assert_eq!(out.segments[0].end, 1.5);
        assert_eq!(out.segments[0].t, "00:00:00.000");
        assert_eq!(out.segments[1].start, 1.5);
        assert_eq!(out.segments[1].end, 3.0);
    }

    #[test]
    fn parses_timestamps_form_with_comma_and_dot() {
        let raw = r#"{
            "transcription": [
              {"text": "ok", "timestamps": {"from": "00:00:01,250", "to": "00:00:02.500"}}
            ]
        }"#;
        let out = parse_whisper_json(raw).unwrap();
        assert_eq!(out.segments.len(), 1);
        assert!((out.segments[0].start - 1.250).abs() < 1e-9);
        assert!((out.segments[0].end - 2.500).abs() < 1e-9);
    }

    #[test]
    fn empty_text_segments_are_dropped() {
        let raw = r#"{
            "transcription": [
              {"text": "  ", "offsets": {"from": 0, "to": 100}},
              {"text": "ok", "offsets": {"from": 100, "to": 200}}
            ]
        }"#;
        let out = parse_whisper_json(raw).unwrap();
        assert_eq!(out.segments.len(), 1);
        assert_eq!(out.segments[0].text, "ok");
    }

    #[test]
    fn missing_transcription_array_is_error() {
        let raw = r#"{"params": {"model": "x"}}"#;
        let err = parse_whisper_json(raw).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("transcription"));
    }

    #[test]
    fn segment_without_times_is_error() {
        let raw = r#"{
            "transcription": [{"text": "missing"}]
        }"#;
        let err = parse_whisper_json(raw).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("offsets") || msg.contains("timestamps"));
    }

    #[test]
    fn offsets_take_precedence_over_timestamps() {
        let raw = r#"{
            "transcription": [
              {"text": "x",
               "offsets": {"from": 7000, "to": 8000},
               "timestamps": {"from": "00:01:00,000", "to": "00:01:01,000"}}
            ]
        }"#;
        let out = parse_whisper_json(raw).unwrap();
        assert_eq!(out.segments[0].start, 7.0);
        assert_eq!(out.segments[0].end, 8.0);
    }

    #[test]
    fn parse_whisper_clock_basic() {
        assert!((parse_whisper_clock("00:00:00,000").unwrap() - 0.0).abs() < 1e-9);
        assert!((parse_whisper_clock("00:01:30.500").unwrap() - 90.5).abs() < 1e-9);
        assert!((parse_whisper_clock("01:00:00,000").unwrap() - 3600.0).abs() < 1e-9);
    }

    #[test]
    fn with_json_extension_swaps_correctly() {
        assert_eq!(
            with_json_extension(Path::new("/tmp/foo")),
            PathBuf::from("/tmp/foo.json"),
        );
        assert_eq!(
            with_json_extension(Path::new("/tmp/foo.txt")),
            PathBuf::from("/tmp/foo.json"),
        );
    }
}
