use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct LiveArgs {}

pub async fn run(_args: LiveArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu live: not yet implemented (U7)");
}
