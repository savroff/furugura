use crate::config::Config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct MarkArgs {
    /// Marker text to capture, verbatim, with the current relative timestamp.
    pub text: String,
}

pub async fn run(_args: MarkArgs, _config: &Config) -> Result<()> {
    anyhow::bail!("furu mark: not yet implemented (U6)");
}
