use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Show only meetings within the last <duration> (e.g., 7d, 24h).
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of meetings to show.
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Show all meetings (overrides --limit).
    #[arg(long, conflicts_with = "limit")]
    pub all: bool,
}

pub async fn run(_args: ListArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu list: not yet implemented (U8)");
}
