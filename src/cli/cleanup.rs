use crate::config::Config;
use crate::lifecycle::lockfile::read_active_meeting;
use crate::paths::Paths;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct CleanupArgs {
    /// Specific meeting id to clean up; defaults to any active runtime state.
    pub id: Option<String>,
}

pub async fn run(args: CleanupArgs, _config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let active_meeting_path = paths.active_meeting_file();
    let lockfile_path = paths.lock_file();

    let am = read_active_meeting(&active_meeting_path)?;
    let runtime_dir_to_remove = match (&args.id, am.as_ref()) {
        (Some(id), _) => paths.runtime_meeting_dir(id),
        (None, Some(active)) => active.runtime_dir.clone(),
        (None, None) => {
            println!("no active meeting found at {}", active_meeting_path.display());
            return Ok(());
        }
    };

    if runtime_dir_to_remove.exists() {
        std::fs::remove_dir_all(&runtime_dir_to_remove)?;
        println!("removed runtime dir: {}", runtime_dir_to_remove.display());
    }
    if active_meeting_path.exists() {
        std::fs::remove_file(&active_meeting_path)?;
        println!("removed state file: {}", active_meeting_path.display());
    }
    if lockfile_path.exists() {
        std::fs::remove_file(&lockfile_path)?;
        println!("removed lockfile: {}", lockfile_path.display());
    }
    Ok(())
}
