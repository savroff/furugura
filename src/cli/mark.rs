use crate::config::Config;
use crate::lifecycle::lockfile::{LifecycleState, read_active_meeting};
use crate::lifecycle::uds::{OkPayload, Request, Response};
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::Args;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

#[derive(Args, Debug)]
pub struct MarkArgs {
    /// Marker text to capture, verbatim, with the current relative timestamp.
    pub text: String,
}

pub async fn run(args: MarkArgs, _config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let am = read_active_meeting(&paths.active_meeting_file())?
        .ok_or_else(|| anyhow!("no active meeting"))?;

    if am.state != LifecycleState::Capturing {
        return Err(anyhow!(
            "meeting is {} — marks are only accepted while capturing",
            am.state.as_str(),
        ));
    }

    // Path safety: canonicalize the notes path and assert it's under
    // the runtime root. Defends against TOCTOU lockfile manipulation.
    let runtime_root = paths.runtime_dir.canonicalize().with_context(|| {
        format!("could not canonicalize runtime dir {}", paths.runtime_dir.display())
    })?;
    let notes_path = canonicalize_with_check(&am.notes_path, &runtime_root)?;

    let elapsed_seconds = (Utc::now() - am.started_at).num_seconds().max(0);
    let t = format_hms(elapsed_seconds);

    let socket = am.runtime_dir.join("furugura.sock");
    match send_via_uds(&socket, &args.text, &t).await {
        Ok(()) => {
            println!("marked: [{t}] {}", args.text);
            Ok(())
        }
        Err(e) => {
            // UDS unreachable — fall back to direct file append. The plan
            // calls this out as the v1 fallback for "rare" socket loss.
            eprintln!("warning: UDS unavailable ({e}); appending directly to notes");
            append_directly(&notes_path, &t, &args.text)?;
            println!("marked (fallback): [{t}] {}", args.text);
            Ok(())
        }
    }
}

/// Canonicalize `path` and assert that it is a strict prefix-child of
/// `runtime_root`. Rejects anything that escapes (e.g. via symlink or a
/// maliciously rewritten lockfile).
fn canonicalize_with_check(path: &Path, runtime_root: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().with_context(|| {
        format!("could not canonicalize notes path {}", path.display())
    })?;
    if !canonical.starts_with(runtime_root) {
        return Err(anyhow!(
            "notes path {} escapes runtime root {} — refusing to write",
            canonical.display(),
            runtime_root.display(),
        ));
    }
    Ok(canonical)
}

pub fn format_hms(elapsed_seconds: i64) -> String {
    let h = elapsed_seconds / 3600;
    let m = (elapsed_seconds % 3600) / 60;
    let s = elapsed_seconds % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

async fn send_via_uds(socket: &Path, text: &str, t: &str) -> Result<()> {
    let mut stream = timeout(Duration::from_millis(200), UnixStream::connect(socket))
        .await
        .map_err(|_| anyhow!("connect timeout"))?
        .with_context(|| format!("could not connect to {}", socket.display()))?;

    let req = Request::Mark {
        text: text.to_string(),
        t: t.to_string(),
    };
    let body = serde_json::to_string(&req)? + "\n";
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream).lines();
    let line = match timeout(Duration::from_secs(2), reader.next_line()).await {
        Ok(Ok(Some(l))) => l,
        Ok(Ok(None)) => return Err(anyhow!("orchestrator closed UDS without ack")),
        Ok(Err(e)) => return Err(anyhow!("UDS read error: {e}")),
        Err(_) => return Err(anyhow!("ack timeout")),
    };
    let resp: Response = serde_json::from_str(line.trim())
        .with_context(|| format!("bad ack: {line}"))?;
    match resp {
        Response::Ok(OkPayload::MarkAck { .. }) => Ok(()),
        Response::Ok(other) => Err(anyhow!("unexpected ack: {other:?}")),
        Response::Err { reason } => Err(anyhow!("orchestrator refused: {reason}")),
    }
}

fn append_directly(notes_path: &Path, t: &str, text: &str) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(notes_path)
        .with_context(|| format!("could not open {} for append", notes_path.display()))?;
    writeln!(f, "[{t}] {text}")?;
    f.sync_data()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn format_hms_zero() {
        assert_eq!(format_hms(0), "00:00:00");
    }

    #[test]
    fn format_hms_under_minute() {
        assert_eq!(format_hms(7), "00:00:07");
    }

    #[test]
    fn format_hms_minutes_and_seconds() {
        assert_eq!(format_hms(83), "00:01:23");
    }

    #[test]
    fn format_hms_over_hour() {
        assert_eq!(format_hms(3661), "01:01:01");
    }

    #[test]
    fn canonicalize_check_accepts_child_path() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let inner = root.join("notes.live");
        std::fs::write(&inner, "").unwrap();
        let result = canonicalize_with_check(&inner, &root).unwrap();
        assert!(result.starts_with(&root));
    }

    #[test]
    fn canonicalize_check_rejects_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let outside = std::env::temp_dir().canonicalize().unwrap();
        let outside_file = outside.join("not-our-file");
        std::fs::write(&outside_file, "").unwrap();
        let err = canonicalize_with_check(&outside_file, &root).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("escapes runtime root"));
        let _ = std::fs::remove_file(&outside_file);
    }

    #[test]
    fn append_directly_writes_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.live");
        std::fs::write(&path, "").unwrap();
        append_directly(&path, "00:01:23", "hello world").unwrap();
        append_directly(&path, "00:02:45", "second mark").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "[00:01:23] hello world\n[00:02:45] second mark\n");
    }
}
