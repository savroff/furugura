use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Skip interactive prompts; only verify dependencies and exit non-zero on missing pieces.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(_args: SetupArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu setup: not yet implemented (U4)");
}
