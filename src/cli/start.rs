use crate::config::Config;
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct StartArgs {
    /// Retain the audio file alongside the markdown after finalization.
    #[arg(long)]
    pub keep_audio: bool,

    /// Override the output directory for this meeting.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    /// Optional human-readable title; otherwise derived from the slug.
    #[arg(long)]
    pub title: Option<String>,

    /// Comma-separated attendee identifiers (e.g., emails).
    #[arg(long, value_delimiter = ',')]
    pub attendees: Vec<String>,

    /// Verify deps without actually starting capture.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(_args: StartArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu start: not yet implemented (U5)");
}
