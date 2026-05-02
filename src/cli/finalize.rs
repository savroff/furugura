use crate::audio::capture::CaptureQuality;
use crate::config::Config;
use crate::lifecycle::lockfile::{LifecycleState, read_active_meeting, transition_state};
use crate::lifecycle::orchestrator::{StartOptions, finalize};
use crate::paths::Paths;
use anyhow::{Result, anyhow};
use clap::Args;

#[derive(Args, Debug)]
pub struct FinalizeArgs {
    /// Meeting id whose retained audio should be re-finalized.
    pub id: String,

    /// Abort an in-progress finalize and discard the runtime state.
    #[arg(long)]
    pub abort: bool,
}

pub async fn run(args: FinalizeArgs, config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let active_meeting_path = paths.active_meeting_file();
    let lockfile_path = paths.lock_file();

    if args.abort {
        if let Some(am) = read_active_meeting(&active_meeting_path)?
            && am.id == args.id
        {
            let _ = std::fs::remove_dir_all(&am.runtime_dir);
            let _ = std::fs::remove_file(&active_meeting_path);
            let _ = std::fs::remove_file(&lockfile_path);
            println!("aborted: removed runtime state for {}", args.id);
            return Ok(());
        }
        return Err(anyhow!(
            "no matching active meeting for id `{}` to abort",
            args.id,
        ));
    }

    let am = read_active_meeting(&active_meeting_path)?
        .ok_or_else(|| anyhow!("no active meeting state at {}", active_meeting_path.display()))?;
    if am.id != args.id {
        return Err(anyhow!(
            "active meeting id is `{}`, not `{}`",
            am.id,
            args.id,
        ));
    }

    println!("re-running finalize for {}", am.id);
    transition_state(&active_meeting_path, LifecycleState::Finalizing).ok();

    let opts = StartOptions {
        keep_audio: am.audio_kept,
        output_dir: Some(am.output_dir.parent().unwrap_or(&am.output_dir).to_path_buf()),
        title: None,
        attendees: am.attendees.clone(),
        cloud_model: None,
        i_have_consent: false,
        yes: true,
    };
    finalize(&am, CaptureQuality::Clean, config, &opts).await?;

    transition_state(&active_meeting_path, LifecycleState::Done).ok();
    let _ = std::fs::remove_file(&active_meeting_path);
    let _ = std::fs::remove_file(&lockfile_path);
    let _ = std::fs::remove_dir_all(&am.runtime_dir);
    println!("finalize complete: {}", am.output_dir.display());
    Ok(())
}
