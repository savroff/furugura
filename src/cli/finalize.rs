use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct FinalizeArgs {
    /// Meeting id whose retained audio should be re-finalized.
    pub id: String,

    /// Abort an in-progress finalize and discard the runtime state.
    #[arg(long)]
    pub abort: bool,
}

pub async fn run(_args: FinalizeArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu finalize: not yet implemented (U5)");
}
