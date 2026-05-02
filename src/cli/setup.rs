//! `furu setup` — verify external dependencies, walk the user through HF
//! token acquisition, persist the token at `~/.config/furugura/hf_token`
//! mode 0600, install the pyannote_runner script, and detect Vulkan.
//!
//! Idempotent: re-running skips already-completed steps. With `--check`,
//! exits non-zero if any dependency is missing and skips interactive prompts.

use crate::config::Config;
use crate::paths::{Paths, ensure_dir};
use anyhow::{Context, Result, anyhow};
use clap::Args;
use std::io::{BufRead, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Skip interactive prompts; only verify dependencies and exit non-zero
    /// on missing pieces.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(args: SetupArgs, config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    ensure_dir(&paths.config_dir)?;

    println!("furugura setup — verifying dependencies");
    println!();

    let report = run_dep_checks().await;
    print_dep_report(&report);

    let any_missing = report.iter().any(|c| !c.ok);
    if any_missing && args.check {
        println!();
        return Err(anyhow!("one or more dependencies missing — see hints above"));
    }

    println!();
    install_pyannote_runner_if_present(&paths)?;
    println!();

    let token_path = config.hf_token_path_or(&paths);
    if token_path.exists() {
        println!("HuggingFace token already at {} — skipping prompt", token_path.display());
    } else if args.check {
        println!("HuggingFace token missing at {} (run without --check to enroll)", token_path.display());
    } else {
        prompt_and_save_hf_token(&token_path)?;
    }

    println!();
    if any_missing {
        println!("setup finished with missing dependencies — install the listed packages and re-run");
        Err(anyhow!("setup incomplete"))
    } else {
        println!("setup complete");
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepReport {
    pub name: &'static str,
    pub ok: bool,
    /// Install hint shown when missing.
    pub hint: &'static str,
}

/// Static dep -> (probe, install hint) table. Defined in code for clarity.
fn dep_specs() -> Vec<(&'static str, &'static [&'static str], &'static str)> {
    vec![
        ("pw-record",       &["pw-record", "--help"],
            "pacman -S pipewire (PipeWire is the v1 audio target)"),
        ("pactl",           &["pactl", "--version"],
            "pacman -S libpulse"),
        ("ffmpeg",          &["ffmpeg", "-version"],
            "pacman -S ffmpeg"),
        ("whisper-cli",     &["whisper-cli", "--help"],
            "paru -S whisper.cpp-vulkan (or build whisper.cpp from source)"),
        ("whisper-stream",  &["whisper-stream", "--help"],
            "paru -S whisper.cpp-vulkan (ships whisper-stream alongside whisper-cli)"),
        ("ollama",          &["ollama", "--version"],
            "pacman -S ollama then `systemctl --user enable --now ollama`"),
        ("python",          &["python", "-c", "import sys"],
            "pacman -S python"),
        ("pyannote.audio",  &["python", "-c", "import pyannote.audio"],
            "pipx install pyannote.audio (or pip install --user pyannote.audio)"),
        ("vulkaninfo",      &["vulkaninfo", "--summary"],
            "pacman -S vulkan-tools (and one of: vulkan-intel, vulkan-radeon, nvidia-utils)"),
    ]
}

pub async fn run_dep_checks() -> Vec<DepReport> {
    let specs = dep_specs();
    let mut out = Vec::with_capacity(specs.len());
    for (name, argv, hint) in specs {
        let ok = probe(argv).await;
        out.push(DepReport { name, ok, hint });
    }
    out
}

async fn probe(argv: &[&str]) -> bool {
    if argv.is_empty() {
        return false;
    }
    let mut cmd = Command::new(argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    matches!(cmd.status().await, Ok(s) if s.success())
}

fn print_dep_report(report: &[DepReport]) {
    for c in report {
        let mark = if c.ok { "OK    " } else { "MISSING" };
        println!("  [{mark}] {}", c.name);
        if !c.ok {
            println!("           hint: {}", c.hint);
        }
    }
}

fn install_pyannote_runner_if_present(_paths: &Paths) -> Result<()> {
    // Look for scripts/install_pyannote_runner.sh next to the binary, OR
    // in the project source tree (during development). If neither is found,
    // emit a one-line note and continue.
    let candidates = pyannote_install_script_candidates();
    let script = candidates.iter().find(|p| p.exists());
    match script {
        Some(s) => {
            println!("running pyannote runner installer: {}", s.display());
            let status = std::process::Command::new("bash")
                .arg(s)
                .status()
                .with_context(|| format!("could not exec {}", s.display()))?;
            if !status.success() {
                return Err(anyhow!("pyannote runner installer exited {status}"));
            }
        }
        None => {
            println!(
                "note: install_pyannote_runner.sh not found — copy scripts/pyannote_runner \
                 to ~/.local/bin manually if `furu stop` reports it missing."
            );
        }
    }
    Ok(())
}

fn pyannote_install_script_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        out.push(dir.join("scripts/install_pyannote_runner.sh"));
        // walk up a few levels to find a `scripts/` sibling (during cargo run / dev).
        for ancestor in dir.ancestors().take(6) {
            let candidate = ancestor.join("scripts/install_pyannote_runner.sh");
            if !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join("scripts/install_pyannote_runner.sh"));
    }
    out
}

fn prompt_and_save_hf_token(target: &Path) -> Result<()> {
    println!("HuggingFace token enrollment");
    println!("  1. Visit https://huggingface.co/pyannote/speaker-diarization-3.1 and accept the user agreement.");
    println!("  2. Visit https://hf.co/settings/tokens and create a fine-grained read token (any name).");
    println!("  3. Paste the token below. (Empty input cancels.)");
    println!();

    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    for attempt in 0..3 {
        print!("HF token: ");
        std::io::stdout().flush()?;
        let mut buf = String::new();
        lock.read_line(&mut buf)?;
        let token = buf.trim();
        match validate_hf_token(token) {
            Ok(()) => {
                save_token_0600(target, token)?;
                println!("token saved to {} (mode 0600)", target.display());
                return Ok(());
            }
            Err(e) => {
                eprintln!("invalid token (attempt {}/3): {e}", attempt + 1);
            }
        }
    }
    Err(anyhow!("HF token not provided after 3 attempts — re-run `furu setup` when ready"))
}

/// Token validation rules:
/// - non-empty after trimming
/// - no whitespace inside the token
/// - reasonable length (>= 8 chars) to catch obvious paste typos
pub fn validate_hf_token(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(anyhow!("empty token"));
    }
    if s.chars().any(char::is_whitespace) {
        return Err(anyhow!("token contains whitespace"));
    }
    if s.len() < 8 {
        return Err(anyhow!("token is implausibly short ({} chars)", s.len()));
    }
    Ok(())
}

fn save_token_0600(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("could not open {} for write", path.display()))?;
        f.write_all(token.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()?;
    }
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("could not chmod 0600: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_token_rejects_empty() {
        assert!(validate_hf_token("").is_err());
    }

    #[test]
    fn validate_token_rejects_whitespace() {
        assert!(validate_hf_token("hf_with space").is_err());
        assert!(validate_hf_token("hf_token\n").is_err());
        assert!(validate_hf_token("\thf_token").is_err());
    }

    #[test]
    fn validate_token_rejects_too_short() {
        assert!(validate_hf_token("abc").is_err());
        assert!(validate_hf_token("1234567").is_err());
    }

    #[test]
    fn validate_token_accepts_typical_hf_format() {
        // hf_ prefix + 32+ random chars is the canonical shape.
        assert!(validate_hf_token("hf_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").is_ok());
        assert!(validate_hf_token("hf_abc12345").is_ok());
    }

    #[test]
    fn save_token_writes_mode_0600() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hf_token");
        save_token_0600(&path, "hf_xxxxxxxxxxxxxxxx").unwrap();
        let mode = std::fs::metadata(&path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "token file should be mode 0600, got {mode:o}");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("hf_xxxxxxxxxxxxxxxx"));
    }

    #[test]
    fn save_token_creates_parent_dir_with_mode_0700() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("nested/cfg");
        let path = parent.join("hf_token");
        save_token_0600(&path, "hf_xxxxxxxxxxxxxxxx").unwrap();
        assert!(path.exists());
        let parent_mode = std::fs::metadata(&parent)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(parent_mode, 0o700);
    }

    #[test]
    fn save_token_overwrites_existing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hf_token");
        save_token_0600(&path, "first_token_value_xx").unwrap();
        save_token_0600(&path, "second_token_value_yy").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("second_token_value_yy"));
        // Mode must persist after overwrite.
        let mode = std::fs::metadata(&path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn dep_specs_are_well_formed() {
        for (name, argv, hint) in dep_specs() {
            assert!(!name.is_empty(), "dep name empty");
            assert!(!argv.is_empty(), "dep argv empty for {name}");
            assert!(!hint.is_empty(), "install hint empty for {name}");
        }
    }
}
