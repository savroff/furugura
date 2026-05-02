use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct StopArgs {
    /// Wait for finalization to complete before returning.
    #[arg(long)]
    pub wait: bool,
}

pub async fn run(_args: StopArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu stop: not yet implemented (U5)");
}
