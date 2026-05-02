use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const APP_QUALIFIER: &str = "";
const APP_ORG: &str = "";
const APP_NAME: &str = "furugura";

/// Filesystem locations Furugura reads and writes.
///
/// All paths are derived from XDG environment variables (with sensible
/// defaults) and from `$HOME`. We do not create directories here — call
/// `ensure_dir` when a path is about to be written.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `~/.config/furugura/` — config + tokens.
    pub config_dir: PathBuf,
    /// `~/.local/share/furugura/` — anything we ever cache durably.
    pub data_dir: PathBuf,
    /// `$XDG_RUNTIME_DIR/furugura/` — tmpfs, ephemeral per-meeting state.
    pub runtime_dir: PathBuf,
    /// `~/Meetings/` — durable per-meeting markdown output (configurable).
    pub default_output_dir: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let dirs = directories::ProjectDirs::from(APP_QUALIFIER, APP_ORG, APP_NAME)
            .context("could not resolve XDG project directories (no $HOME?)")?;

        let runtime_dir = runtime_dir().context("could not resolve $XDG_RUNTIME_DIR")?;
        let home = home_dir().context("could not resolve $HOME")?;

        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            runtime_dir: runtime_dir.join(APP_NAME),
            default_output_dir: home.join("Meetings"),
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn hf_token_file(&self) -> PathBuf {
        self.config_dir.join("hf_token")
    }

    pub fn anthropic_token_file(&self) -> PathBuf {
        self.config_dir.join("anthropic_token")
    }

    pub fn active_meeting_file(&self) -> PathBuf {
        self.runtime_dir.join("active-meeting.json")
    }

    pub fn runtime_meeting_dir(&self, id: &str) -> PathBuf {
        self.runtime_dir.join(id)
    }
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Create `dir` (and parents) with mode 0700 if it does not already exist.
pub fn ensure_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)
        .with_context(|| format!("could not create directory: {}", dir.display()))?;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(dir, perms)
        .with_context(|| format!("could not chmod 0700: {}", dir.display()))?;
    Ok(())
}
