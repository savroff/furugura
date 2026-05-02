//! Standalone pyannote.audio diarization pass at finalize.
//!
//! Spawns a thin Python wrapper (`pyannote_runner`, installed by `furu setup`)
//! that loads `pyannote/speaker-diarization-3.1`, runs it against the
//! finalized WAV, and emits RTTM to stdout. The HF token is passed via the
//! subprocess `HF_TOKEN` environment variable — never on the command line —
//! to avoid `/proc/<pid>/cmdline` exposure.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use tokio::process::Command;

use super::rttm::{RttmSegment, parse_rttm};

#[derive(Debug, Clone)]
pub struct DiarizeConfig {
    pub wav_path: PathBuf,
    /// HuggingFace token; passed via env, never CLI flag.
    pub hf_token: String,
    /// Override the wrapper command (default: `pyannote_runner`).
    pub binary: Option<String>,
    /// Optional file_id used to label rows when we re-emit RTTM downstream.
    /// Not passed to the wrapper.
    pub file_id: String,
}

pub struct DiarizationResult {
    pub segments: Vec<RttmSegment>,
    /// Raw RTTM text, suitable for writing to the meeting's `.rttm` sidecar
    /// without re-rendering. May be empty if the wrapper produced none.
    pub rttm_text: String,
}

/// Run the pyannote wrapper and parse its RTTM output.
pub async fn run(cfg: DiarizeConfig) -> Result<DiarizationResult> {
    let bin = cfg.binary.as_deref().unwrap_or("pyannote_runner");
    let mut cmd = Command::new(bin);
    cmd.arg(&cfg.wav_path);
    cmd.env("HF_TOKEN", &cfg.hf_token);
    let out = cmd
        .output()
        .await
        .with_context(|| format!("could not run `{bin}`"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "{bin} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        ));
    }
    let rttm_text = String::from_utf8(out.stdout)
        .context("pyannote wrapper produced non-UTF-8 RTTM")?;
    let segments = parse_rttm(&rttm_text)?;
    Ok(DiarizationResult { segments, rttm_text })
}

/// Path of the diarization sidecar we persist alongside the meeting.
pub fn rttm_sidecar_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.diarization.rttm"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rttm_sidecar_path_uses_id() {
        let dir = tempdir().unwrap();
        let p = rttm_sidecar_path(dir.path(), "2026-05-02-1430-team");
        assert!(p.ends_with("2026-05-02-1430-team.diarization.rttm"));
        assert_eq!(p.parent(), Some(dir.path()));
    }
}
