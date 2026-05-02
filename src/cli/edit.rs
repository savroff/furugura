use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Meeting id, partial-suffix, or shortlist offset (e.g., "1" = most recent).
    pub id: String,
}

pub async fn run(_args: EditArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu edit: not yet implemented (U8)");
}
