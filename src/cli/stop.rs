use crate::config::Config;
use crate::lifecycle::lockfile::{LifecycleState, read_active_meeting};
use crate::lifecycle::uds::{OkPayload, Request, Response, render_response};
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use clap::Args;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

#[derive(Args, Debug)]
pub struct StopArgs {}

pub async fn run(_args: StopArgs, _config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let am = read_active_meeting(&paths.active_meeting_file())?
        .ok_or_else(|| anyhow!("no active meeting"))?;

    let socket_path = am.runtime_dir.join("furugura.sock");

    // If meeting is already finalizing, print progress and exit.
    if am.state != LifecycleState::Capturing {
        println!("meeting is {} (id: {})", am.state.as_str(), am.id);
        let elapsed_since_started = (chrono::Utc::now() - am.started_at).num_seconds();
        println!("  elapsed since start: {elapsed_since_started}s");
        return Ok(());
    }

    let mut stream = timeout(
        Duration::from_millis(500),
        UnixStream::connect(&socket_path),
    )
    .await
    .with_context(|| {
        format!(
            "timed out connecting to {} — is `furu start` still running?",
            socket_path.display(),
        )
    })?
    .with_context(|| format!("could not connect to {}", socket_path.display()))?;

    let req = Request::Stop;
    let body = serde_json::to_string(&req)? + "\n";
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream).lines();
    let line = match timeout(Duration::from_secs(5), reader.next_line()).await {
        Ok(Ok(Some(l))) => l,
        Ok(Ok(None)) => return Err(anyhow!("orchestrator closed UDS without ack")),
        Ok(Err(e)) => return Err(anyhow!("UDS read error: {e}")),
        Err(_) => return Err(anyhow!("timed out waiting for stop ack")),
    };

    let resp: Response = serde_json::from_str(line.trim())
        .with_context(|| format!("bad ack: {line}"))?;
    match resp {
        Response::Ok(OkPayload::StopAck { state }) => {
            println!("stop acknowledged — meeting is {state}");
            println!("(the `furu start` terminal is finalizing in the background)");
            Ok(())
        }
        Response::Ok(other) => {
            // Unexpected payload but not an error.
            println!("stop ack: {}", render_response(&Response::Ok(other)));
            Ok(())
        }
        Response::Err { reason } => Err(anyhow!("stop refused: {reason}")),
    }
}
