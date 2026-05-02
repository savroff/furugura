//! Streaming whisper.cpp pass.
//!
//! Spawns `whisper-stream --model <gguf> --step 500 --length 5000 --keep 200
//! --vad-thold 0.6` reading PCM s16 stereo from stdin (fed by U2's broadcast
//! tee — downmixed to mono on the way in, since whisper-stream takes a single
//! channel). Parses whisper-stream's stdout into JSONL segments and appends
//! them to `transcript.live.jsonl`.

use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use super::jsonl::{LiveSegment, append_live_segment, format_timestamp};
use crate::audio::capture::StereoBatch;

#[derive(Debug, Clone)]
pub struct WhisperStreamConfig {
    pub model_path: PathBuf,
    pub jsonl_path: PathBuf,
    pub sample_rate: u32,
    pub binary: Option<String>,
}

pub struct WhisperStreamHandle {
    child: Arc<Mutex<Child>>,
    feeder: JoinHandle<()>,
    parser: JoinHandle<()>,
}

impl WhisperStreamHandle {
    pub async fn shutdown(self) -> Result<()> {
        // Drop the feeder so its stdin closes; whisper-stream will EOF.
        self.feeder.abort();
        let _ = self.feeder.await;
        // Give the parser a moment to drain, then abort.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            wait_for_handle(&self.parser),
        )
        .await;
        self.parser.abort();
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = child.wait().await;
        Ok(())
    }
}

async fn wait_for_handle(_h: &JoinHandle<()>) {
    // Placeholder — JoinHandle::await consumes by value; the parser will
    // exit naturally when its stdout reader sees EOF.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

/// Spawn whisper-stream and wire its stdin/stdout to the broadcast tee
/// and the JSONL writer.
pub async fn start(
    cfg: WhisperStreamConfig,
    rx: broadcast::Receiver<StereoBatch>,
) -> Result<WhisperStreamHandle> {
    let bin = cfg.binary.as_deref().unwrap_or("whisper-stream").to_string();

    let mut cmd = Command::new(&bin);
    cmd.arg("--model").arg(&cfg.model_path);
    cmd.arg("--step").arg("500");
    cmd.arg("--length").arg("5000");
    cmd.arg("--keep").arg("200");
    cmd.arg("--vad-thold").arg("0.6");
    cmd.arg("--file").arg("-"); // stdin
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .with_context(|| format!("could not spawn `{bin}` (is it on $PATH?)"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("whisper-stream stdin vanished"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("whisper-stream stdout vanished"))?;

    let feeder = tokio::spawn(feed_stdin(rx, stdin));
    let parser = tokio::spawn(parse_stdout(stdout, cfg.jsonl_path.clone()));

    Ok(WhisperStreamHandle {
        child: Arc::new(Mutex::new(child)),
        feeder,
        parser,
    })
}

/// Read interleaved-stereo PCM batches from the broadcast and write
/// downmixed mono s16 LE bytes to whisper-stream's stdin.
async fn feed_stdin(
    mut rx: broadcast::Receiver<StereoBatch>,
    mut stdin: tokio::process::ChildStdin,
) {
    loop {
        match rx.recv().await {
            Ok(batch) => {
                let mono = downmix_stereo_to_mono(&batch);
                let bytes: Vec<u8> = mono
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                if stdin.write_all(&bytes).await.is_err() {
                    debug!("whisper-stream stdin closed");
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("whisper-stream feeder lagged {n} batches");
            }
        }
    }
    let _ = stdin.shutdown().await;
}

fn downmix_stereo_to_mono(interleaved: &[i16]) -> Vec<i16> {
    let mut out = Vec::with_capacity(interleaved.len() / 2);
    for chunk in interleaved.chunks_exact(2) {
        let l = chunk[0] as i32;
        let r = chunk[1] as i32;
        out.push(((l + r) / 2) as i16);
    }
    out
}

/// Read whisper-stream's stdout line-by-line. Lines that match the
/// whisper-stream output pattern are appended to `transcript.live.jsonl`.
///
/// whisper-stream's text output looks like:
///   `[00:00:01.000 --> 00:00:03.500]  Hello world`
async fn parse_stdout(stdout: tokio::process::ChildStdout, jsonl_path: PathBuf) {
    let mut reader = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        if let Some(seg) = parse_stream_line(&line) {
            if let Err(e) = append_live_segment(&jsonl_path, &seg) {
                warn!("could not append live segment: {e}");
            }
        }
    }
}

/// Parse a whisper-stream stdout line into a `LiveSegment`. Returns
/// `None` for lines that aren't transcript segments (banners, status).
pub fn parse_stream_line(line: &str) -> Option<LiveSegment> {
    let line = line.trim_start();
    let line = line.strip_prefix('[')?;
    let bracket = line.find(']')?;
    let (range, rest) = line.split_at(bracket);
    let text = rest.trim_start_matches(']').trim();
    if text.is_empty() {
        return None;
    }
    let mut parts = range.split("-->");
    let from = parts.next()?.trim();
    let to = parts.next()?.trim();
    let start = parse_clock(from)?;
    let end = parse_clock(to)?;
    Some(LiveSegment {
        t: format_timestamp(start),
        speaker: None,
        text: text.to_string(),
        start,
        end,
    })
}

fn parse_clock(s: &str) -> Option<f64> {
    let s = s.replace(',', ".");
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_stream_line() {
        let line = "[00:00:01.000 --> 00:00:03.500]  Hello world";
        let seg = parse_stream_line(line).unwrap();
        assert!((seg.start - 1.0).abs() < 1e-9);
        assert!((seg.end - 3.5).abs() < 1e-9);
        assert_eq!(seg.text, "Hello world");
        assert_eq!(seg.t, "00:00:01.000");
    }

    #[test]
    fn handles_comma_decimal_separator() {
        let line = "[00:00:01,250 --> 00:00:02,500]  test";
        let seg = parse_stream_line(line).unwrap();
        assert!((seg.start - 1.25).abs() < 1e-9);
        assert!((seg.end - 2.5).abs() < 1e-9);
    }

    #[test]
    fn ignores_non_transcript_lines() {
        assert!(parse_stream_line("init: loading model...").is_none());
        assert!(parse_stream_line("").is_none());
        assert!(parse_stream_line("[00:00:01.000 --> 00:00:03.500] ").is_none());
    }

    #[test]
    fn downmix_averages_channels() {
        let interleaved = vec![10i16, 30, 100, 200];
        let mono = downmix_stereo_to_mono(&interleaved);
        assert_eq!(mono, vec![20, 150]);
    }

    #[test]
    fn downmix_avoids_overflow() {
        let interleaved = vec![i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        let mono = downmix_stereo_to_mono(&interleaved);
        assert_eq!(mono, vec![i16::MAX, i16::MIN]);
    }

    #[test]
    fn downmix_drops_odd_trailing_sample() {
        let interleaved = vec![10i16, 20, 30];
        let mono = downmix_stereo_to_mono(&interleaved);
        assert_eq!(mono, vec![15]);
    }
}
