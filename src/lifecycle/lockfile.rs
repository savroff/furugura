//! Lockfile + active-meeting state.
//!
//! `$XDG_RUNTIME_DIR/furugura/active-meeting.json` is the single source of
//! truth for "is a meeting in progress, and where is its state". A
//! `flock(LOCK_EX | LOCK_NB)` on that file gates `furu start`; the lock
//! is held by the orchestrator process for the meeting's whole lifetime
//! (including `finalizing`). Other commands read the file without locking.
//!
//! State transitions:
//!   `capturing → finalizing → done`
//!
//! On `done`, the orchestrator releases the lock and removes the lockfile.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    Capturing,
    Finalizing,
    Done,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleState::Capturing => "capturing",
            LifecycleState::Finalizing => "finalizing",
            LifecycleState::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveMeeting {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub state: LifecycleState,
    pub notes_path: PathBuf,
    pub runtime_dir: PathBuf,
    pub output_dir: PathBuf,
    pub audio_kept: bool,
    pub attendees: Vec<String>,
}

/// RAII guard for the held flock. Drop releases the lock automatically.
/// The underlying file is kept open while the guard is alive.
pub struct LockGuard {
    _flock: Flock<File>,
    pub path: PathBuf,
}

impl std::fmt::Debug for LockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LockGuard").field("path", &self.path).finish()
    }
}

/// Acquire `LOCK_EX | LOCK_NB`. Errors when another orchestrator already
/// holds it. Caller is responsible for filling the file with
/// `write_active_meeting` afterward.
pub fn acquire_lock(path: &Path) -> Result<LockGuard> {
    if let Some(parent) = path.parent() {
        crate::paths::ensure_dir(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("could not open lockfile {}", path.display()))?;
    let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(_, e)| {
        anyhow!(
            "another meeting is in progress: {} ({e})",
            path.display(),
        )
    })?;
    Ok(LockGuard {
        _flock: lock,
        path: path.to_path_buf(),
    })
}

/// Atomically write the active-meeting JSON. Creates a temp file in the
/// same directory and renames into place.
pub fn write_active_meeting(path: &Path, am: &ActiveMeeting) -> Result<()> {
    let parent = path.parent().ok_or_else(|| anyhow!("lockfile has no parent"))?;
    crate::paths::ensure_dir(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("could not create temp file in {}", parent.display()))?;
    let bytes = serde_json::to_vec_pretty(am)?;
    tmp.write_all(&bytes)?;
    tmp.write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| anyhow!("could not persist lockfile: {}", e.error))?;
    Ok(())
}

/// Read without holding the lock. Returns `Ok(None)` when the file does
/// not exist (no meeting in progress).
pub fn read_active_meeting(path: &Path) -> Result<Option<ActiveMeeting>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let am: ActiveMeeting = serde_json::from_str(&raw)
        .with_context(|| format!("invalid active-meeting JSON at {}", path.display()))?;
    Ok(Some(am))
}

/// Mutate the state field in place, then atomically rewrite the file.
/// Validates the transition: `capturing → finalizing → done` only.
pub fn transition_state(path: &Path, new_state: LifecycleState) -> Result<ActiveMeeting> {
    let mut am = read_active_meeting(path)?
        .ok_or_else(|| anyhow!("no active meeting at {}", path.display()))?;
    if !is_legal_transition(am.state, new_state) {
        return Err(anyhow!(
            "illegal state transition: {} → {}",
            am.state.as_str(),
            new_state.as_str(),
        ));
    }
    am.state = new_state;
    write_active_meeting(path, &am)?;
    Ok(am)
}

pub fn is_legal_transition(from: LifecycleState, to: LifecycleState) -> bool {
    matches!(
        (from, to),
        (LifecycleState::Capturing, LifecycleState::Finalizing)
            | (LifecycleState::Finalizing, LifecycleState::Done)
            // Idempotent: re-asserting the same state is legal (no-op).
            | (LifecycleState::Capturing, LifecycleState::Capturing)
            | (LifecycleState::Finalizing, LifecycleState::Finalizing)
            | (LifecycleState::Done, LifecycleState::Done)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(dir: &Path) -> ActiveMeeting {
        ActiveMeeting {
            id: "2026-05-02-1430-test".into(),
            started_at: Utc::now(),
            state: LifecycleState::Capturing,
            notes_path: dir.join("notes.live"),
            runtime_dir: dir.to_path_buf(),
            output_dir: dir.join("out"),
            audio_kept: false,
            attendees: vec![],
        }
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let am = sample(dir.path());
        write_active_meeting(&path, &am).unwrap();
        let read = read_active_meeting(&path).unwrap().unwrap();
        assert_eq!(read.id, am.id);
        assert_eq!(read.state, LifecycleState::Capturing);
    }

    #[test]
    fn read_missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json");
        assert!(read_active_meeting(&path).unwrap().is_none());
    }

    #[test]
    fn legal_transitions() {
        assert!(is_legal_transition(LifecycleState::Capturing, LifecycleState::Finalizing));
        assert!(is_legal_transition(LifecycleState::Finalizing, LifecycleState::Done));
        // Idempotent same-state.
        assert!(is_legal_transition(LifecycleState::Capturing, LifecycleState::Capturing));
    }

    #[test]
    fn illegal_transitions_rejected() {
        assert!(!is_legal_transition(LifecycleState::Done, LifecycleState::Capturing));
        assert!(!is_legal_transition(LifecycleState::Done, LifecycleState::Finalizing));
        assert!(!is_legal_transition(LifecycleState::Finalizing, LifecycleState::Capturing));
        assert!(!is_legal_transition(LifecycleState::Capturing, LifecycleState::Done));
    }

    #[test]
    fn transition_state_updates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let am = sample(dir.path());
        write_active_meeting(&path, &am).unwrap();

        let updated = transition_state(&path, LifecycleState::Finalizing).unwrap();
        assert_eq!(updated.state, LifecycleState::Finalizing);
        let read_back = read_active_meeting(&path).unwrap().unwrap();
        assert_eq!(read_back.state, LifecycleState::Finalizing);
    }

    #[test]
    fn transition_state_rejects_illegal() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let am = sample(dir.path());
        write_active_meeting(&path, &am).unwrap();

        let err = transition_state(&path, LifecycleState::Done).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("illegal state transition"));
    }

    #[test]
    fn flock_blocks_second_acquirer() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let _g = acquire_lock(&path).unwrap();
        let err = acquire_lock(&path).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("another meeting is in progress"),
            "expected lock-conflict message, got: {msg}",
        );
    }

    #[test]
    fn flock_releases_on_drop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        {
            let _g = acquire_lock(&path).unwrap();
        } // drop releases
        // Second acquire should succeed.
        let _g2 = acquire_lock(&path).unwrap();
    }
}
