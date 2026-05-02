//! Dual-`pw-record` capture orchestrator.
//!
//! Spawns one subprocess per channel (mic + system monitor), reads
//! 16-bit-LE PCM mono off each stdout, interleaves to stereo, and tees
//! into a `tokio::sync::broadcast` channel + the always-on tmpfs WAV.
//!
//! Sample-clock alignment is enforced by both subprocesses being locked
//! to the same `--rate`. Drift is detected by counting samples emitted
//! per channel and comparing against wall-clock elapsed time; if the
//! delta exceeds a threshold, capture quality is flagged `Degraded` and
//! surfaced to the lifecycle layer for `capture_quality:` frontmatter.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use super::interleave::{decode_le_s16, interleave_stereo_i16};
use super::wav_writer::StereoWavWriter;

/// Drift threshold (in milliseconds of elapsed audio) above which capture
/// quality is reported as degraded. The plan specifies 50 ms.
const DRIFT_DEGRADED_MS: u64 = 50;

/// How many frames of stereo PCM the broadcast channel will buffer per
/// subscriber. At 48 kHz this is ~10 ms per "frame batch" depending on
/// how the reader chunks; 1024 batches give consumers ~10s of headroom.
const BROADCAST_CAPACITY: usize = 1024;

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// PipeWire / pactl source name for the microphone. `None` → use
    /// PipeWire's currently-default source.
    pub mic_source: Option<String>,
    /// PipeWire / pactl source name for the system-audio monitor.
    /// Resolved via `source_resolve::default_monitor_source` when `None`.
    pub system_source: Option<String>,
    /// Sample rate to lock both subprocesses to (Hz). Plan default: 48 000.
    pub sample_rate: u32,
    /// Path to write the always-on tmpfs WAV.
    pub wav_path: PathBuf,
}

/// Audio integrity state surfaced to the lifecycle layer at `furu stop`.
/// Used to set `capture_quality:` in the frontmatter (R10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureQuality {
    Clean,
    Degraded,
}

/// Stereo PCM batch broadcast to live consumers (whisper-stream).
/// One batch is the result of one read iteration after interleave.
pub type StereoBatch = Arc<Vec<i16>>;

pub struct CaptureHandle {
    /// Subscribe to receive interleaved stereo s16 PCM batches.
    pub broadcast: broadcast::Sender<StereoBatch>,
    /// Resolves to `Clean | Degraded` after the capture loop exits.
    quality_rx: oneshot::Receiver<CaptureQuality>,
    /// Send to request graceful shutdown of capture.
    shutdown_tx: mpsc::Sender<()>,
    /// Handle on the orchestrator task; `await` to surface fatal errors.
    join: JoinHandle<Result<()>>,
}

impl CaptureHandle {
    pub async fn shutdown(self) -> Result<CaptureQuality> {
        // Best-effort signal; receiver may already have exited.
        let _ = self.shutdown_tx.send(()).await;
        let join_result = self.join.await;
        let quality = self
            .quality_rx
            .await
            .unwrap_or(CaptureQuality::Degraded);
        match join_result {
            Ok(Ok(())) => Ok(quality),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(anyhow!("capture task panicked: {e}")),
        }
    }
}

/// Compute drift in milliseconds between expected and actual sample counts,
/// given how much wall-clock time has elapsed.
///
/// Returns `(left_drift_ms, right_drift_ms)`, each absolute.
pub fn compute_drift_ms(
    sample_rate: u32,
    elapsed: Duration,
    left_samples: u64,
    right_samples: u64,
) -> (u64, u64) {
    let expected = (sample_rate as u64) * elapsed.as_millis() as u64 / 1000;
    let l = left_samples.abs_diff(expected) * 1000 / sample_rate as u64;
    let r = right_samples.abs_diff(expected) * 1000 / sample_rate as u64;
    (l, r)
}

/// Decide whether the observed drift trips the degraded-quality bit.
pub fn drift_is_degraded(left_drift_ms: u64, right_drift_ms: u64) -> bool {
    left_drift_ms.max(right_drift_ms) > DRIFT_DEGRADED_MS
}

/// Spawn the capture pipeline. Returns once both `pw-record` subprocesses
/// are running and the orchestrator task is reading from them.
pub async fn start_capture(cfg: CaptureConfig) -> Result<CaptureHandle> {
    if let Some(parent) = cfg.wav_path.parent() {
        crate::paths::ensure_dir(parent)?;
    }

    // pw-record defaults to the system default source if we omit --target.
    // None here means "no --target".
    let mic_source = cfg.mic_source.clone();
    let system_source = match cfg.system_source.clone() {
        Some(s) => s,
        None => super::source_resolve::default_monitor_source().await?,
    };

    let mic_child = spawn_pw_record(mic_source.as_deref(), cfg.sample_rate)
        .context("could not spawn mic pw-record (is pw-record on $PATH?)")?;
    let sys_child = spawn_pw_record(Some(&system_source), cfg.sample_rate)
        .context("could not spawn system-monitor pw-record")?;

    let (broadcast_tx, _) = broadcast::channel::<StereoBatch>(BROADCAST_CAPACITY);
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    let (quality_tx, quality_rx) = oneshot::channel();

    let wav_writer = StereoWavWriter::create(&cfg.wav_path, cfg.sample_rate)
        .with_context(|| {
            format!("could not open WAV at {}", cfg.wav_path.display())
        })?;

    let task_tx = broadcast_tx.clone();
    let join = tokio::spawn(async move {
        run_capture_loop(
            mic_child,
            sys_child,
            wav_writer,
            cfg.sample_rate,
            task_tx,
            shutdown_rx,
            quality_tx,
        )
        .await
    });

    Ok(CaptureHandle {
        broadcast: broadcast_tx,
        quality_rx,
        shutdown_tx,
        join,
    })
}

fn spawn_pw_record(target: Option<&str>, rate: u32) -> Result<Child> {
    let mut cmd = Command::new("pw-record");
    cmd.arg("--rate").arg(rate.to_string());
    cmd.arg("--channels").arg("1");
    cmd.arg("--format").arg("s16");
    cmd.arg("--raw");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    cmd.arg("-"); // stdout
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    let child = cmd.spawn().context("failed to spawn pw-record")?;
    Ok(child)
}

#[allow(clippy::too_many_arguments)]
async fn run_capture_loop(
    mut mic_child: Child,
    mut sys_child: Child,
    mut wav: StereoWavWriter,
    sample_rate: u32,
    broadcast_tx: broadcast::Sender<StereoBatch>,
    mut shutdown_rx: mpsc::Receiver<()>,
    quality_tx: oneshot::Sender<CaptureQuality>,
) -> Result<()> {
    // ~10ms of audio per read at 48 kHz mono s16 = 480 samples = 960 bytes.
    let read_bytes = (sample_rate as usize / 100) * 2;

    let mic_stdout = mic_child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("mic pw-record stdout vanished"))?;
    let sys_stdout = sys_child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("system pw-record stdout vanished"))?;

    let mic_count = Arc::new(AtomicU64::new(0));
    let sys_count = Arc::new(AtomicU64::new(0));

    let (mic_tx, mut mic_rx) = mpsc::channel::<Vec<i16>>(64);
    let (sys_tx, mut sys_rx) = mpsc::channel::<Vec<i16>>(64);

    let mic_count_w = mic_count.clone();
    let sys_count_w = sys_count.clone();

    tokio::spawn(reader_task(mic_stdout, read_bytes, mic_tx, mic_count_w));
    tokio::spawn(reader_task(sys_stdout, read_bytes, sys_tx, sys_count_w));

    let started = Instant::now();
    let mut quality = CaptureQuality::Clean;
    let mut last_check = started;
    let mut interleaved_buf: Vec<i16> = Vec::with_capacity(read_bytes);

    let mut mic_buf: Vec<i16> = Vec::new();
    let mut sys_buf: Vec<i16> = Vec::new();

    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.recv() => {
                debug!("capture shutdown requested");
                break;
            }

            recv = mic_rx.recv() => {
                match recv {
                    Some(samples) => mic_buf.extend(samples),
                    None => {
                        warn!("mic stream EOF — capture stopping");
                        quality = CaptureQuality::Degraded;
                        break;
                    }
                }
            }

            recv = sys_rx.recv() => {
                match recv {
                    Some(samples) => sys_buf.extend(samples),
                    None => {
                        warn!("system stream EOF — capture stopping");
                        quality = CaptureQuality::Degraded;
                        break;
                    }
                }
            }
        }

        let pair_len = mic_buf.len().min(sys_buf.len());
        if pair_len > 0 {
            let mic_chunk: Vec<i16> = mic_buf.drain(..pair_len).collect();
            let sys_chunk: Vec<i16> = sys_buf.drain(..pair_len).collect();
            interleave_stereo_i16(&mic_chunk, &sys_chunk, &mut interleaved_buf);
            wav.write_interleaved(&interleaved_buf)?;
            // broadcast: best-effort. If no subscribers, send_err is fine.
            let _ = broadcast_tx.send(Arc::new(interleaved_buf.clone()));
        }

        if last_check.elapsed() >= Duration::from_secs(5) {
            let elapsed = started.elapsed();
            let m = mic_count.load(Ordering::Relaxed);
            let s = sys_count.load(Ordering::Relaxed);
            let (l_drift, r_drift) = compute_drift_ms(sample_rate, elapsed, m, s);
            if drift_is_degraded(l_drift, r_drift) {
                warn!(
                    "audio clock drift mic={}ms sys={}ms (threshold {}ms) — capture flagged degraded",
                    l_drift, r_drift, DRIFT_DEGRADED_MS,
                );
                quality = CaptureQuality::Degraded;
            }
            last_check = Instant::now();
        }
    }

    // Best-effort: kill subprocesses and finalize the WAV.
    let _ = mic_child.start_kill();
    let _ = sys_child.start_kill();
    let _ = mic_child.wait().await;
    let _ = sys_child.wait().await;
    wav.finalize()?;
    let _ = quality_tx.send(quality);
    Ok(())
}

async fn reader_task(
    mut stdout: tokio::process::ChildStdout,
    read_bytes: usize,
    tx: mpsc::Sender<Vec<i16>>,
    counter: Arc<AtomicU64>,
) {
    let mut byte_buf = vec![0u8; read_bytes];
    let mut leftover: Vec<u8> = Vec::new();
    loop {
        let n = match stdout.read(&mut byte_buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                warn!("pw-record stdout read error: {e}");
                break;
            }
        };

        let mut combined = std::mem::take(&mut leftover);
        combined.extend_from_slice(&byte_buf[..n]);
        let mut samples: Vec<i16> = Vec::with_capacity(combined.len() / 2);
        let consumed_pairs = decode_le_s16(&combined, &mut samples);
        let consumed_bytes = consumed_pairs * 2;
        if consumed_bytes < combined.len() {
            leftover = combined.split_off(consumed_bytes);
        }
        counter.fetch_add(samples.len() as u64, Ordering::Relaxed);
        if tx.send(samples).await.is_err() {
            break;
        }
    }
}

#[allow(dead_code)]
fn _path_marker(_p: &Path) {} // keeps Path import used if reader_task signature ever moves
