use crate::config::Config;
use crate::lifecycle::{StartOptions, start_and_run};
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

    /// Use a cloud LLM (Anthropic) for summarization. Triggers the consent gate.
    #[arg(long, value_name = "MODEL")]
    pub cloud_model: Option<String>,

    /// Acknowledge that named attendees have consented to third-party processing.
    /// Required when `--cloud-model` is set and `--attendees` is non-empty.
    #[arg(long)]
    pub i_have_consent: bool,

    /// Skip the cloud-summary interactive consent prompt (for scripted use).
    #[arg(long)]
    pub yes: bool,

    /// Verify deps without actually starting capture.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(args: StartArgs, config: &Config) -> Result<()> {
    if args.dry_run {
        println!("dry run — verifying deps via `furu setup --check`");
        return crate::cli::setup::run(
            crate::cli::setup::SetupArgs { check: true },
            config,
        )
        .await;
    }

    let opts = StartOptions {
        keep_audio: args.keep_audio,
        output_dir: args.output_dir,
        title: args.title,
        attendees: args.attendees,
        cloud_model: args.cloud_model,
        i_have_consent: args.i_have_consent,
        yes: args.yes,
    };
    start_and_run(config, opts).await
}
