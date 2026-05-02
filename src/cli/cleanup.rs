use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct CleanupArgs {
    /// Specific meeting id to clean up; defaults to any active runtime state.
    pub id: Option<String>,
}

pub async fn run(_args: CleanupArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu cleanup: not yet implemented (U5)");
}
