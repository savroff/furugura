use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub mod cleanup;
pub mod edit;
pub mod finalize;
pub mod list;
pub mod live;
pub mod mark;
pub mod setup;
pub mod start;
pub mod stop;

#[derive(Parser, Debug)]
#[command(
    name = "furu",
    version,
    about = "CLI-only Linux meeting capture, transcription, and AI summary",
    long_about = None,
)]
pub struct Cli {
    /// Path to a config file (overrides ~/.config/furugura/config.toml).
    #[arg(long, global = true, env = "FURU_CONFIG")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Begin capturing a meeting.
    Start(start::StartArgs),

    /// Stop capturing the active meeting.
    Stop(stop::StopArgs),

    /// Capture a timestamped marker note in the active meeting.
    Mark(mark::MarkArgs),

    /// List past meetings.
    List(list::ListArgs),

    /// Open a past meeting in $EDITOR.
    Edit(edit::EditArgs),

    /// Open the live transcript view for the active meeting.
    Live(live::LiveArgs),

    /// Verify dependencies and walk first-run setup.
    Setup(setup::SetupArgs),

    /// Discard stale runtime state.
    Cleanup(cleanup::CleanupArgs),

    /// Re-run the post-stop pipeline against a retained meeting.
    Finalize(finalize::FinalizeArgs),
}

pub async fn run(cli: Cli) -> Result<()> {
    let config = crate::config::Config::load(cli.config.as_deref())?;
    match cli.command {
        Command::Start(args) => start::run(args, &config).await,
        Command::Stop(args) => stop::run(args, &config).await,
        Command::Mark(args) => mark::run(args, &config).await,
        Command::List(args) => list::run(args, &config).await,
        Command::Edit(args) => edit::run(args, &config).await,
        Command::Live(args) => live::run(args, &config).await,
        Command::Setup(args) => setup::run(args, &config).await,
        Command::Cleanup(args) => cleanup::run(args, &config).await,
        Command::Finalize(args) => finalize::run(args, &config).await,
    }
}
