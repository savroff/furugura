//! End-to-end orchestrator: drives `furu start` from "user invoked" through
//! "meeting.md written and lockfile released."
//!
//! Pipeline:
//!   1. id + dirs + lockfile
//!   2. UDS bind (mode 0600)
//!   3. capture pipeline (U2) — always
//!   4. streaming whisper (U3) — best effort, warns if missing
//!   5. UDS server loop until `Stop` or SIGINT
//!   6. transition `capturing → finalizing`
//!   7. shutdown capture + streaming
//!   8. finalize (batch transcribe + diarize + summarize + markdown)
//!   9. transition `finalizing → done`; cleanup runtime state.
//!
//! External binaries that are missing degrade gracefully — the orchestrator
//! still produces a markdown file with whatever it could compute. The
//! frontmatter audit fields make the degradation observable on disk.

use anyhow::{Context, Result, anyhow};
use chrono::{Local, Utc};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::warn;

use super::id::make_id;
use super::lockfile::{
    ActiveMeeting, LifecycleState, acquire_lock, transition_state, write_active_meeting,
};
use super::uds::{OkPayload, Request, Response, parse_request, render_response};
use crate::audio::capture::{CaptureConfig, CaptureQuality, start_capture};
use crate::config::{Config, SummaryProvider};
use crate::output::{MeetingBody, MeetingFrontmatter, assemble, write_meeting_md};
use crate::paths::Paths;
use crate::summarize::{
    ConsentDecision, ConsentInputs, DataEgressed, ProviderKind, SummaryAudit,
    SummaryBlock, anthropic, evaluate_consent, ollama, parse_summary, prompt,
};
use crate::transcribe::{
    LiveSegment, RttmSegment, jsonl, merge_speaker_labels, pyannote,
    whisper_batch, whisper_stream,
};

#[derive(Debug, Clone)]
pub struct StartOptions {
    pub keep_audio: bool,
    pub output_dir: Option<PathBuf>,
    pub title: Option<String>,
    pub attendees: Vec<String>,
    pub cloud_model: Option<String>,
    pub i_have_consent: bool,
    pub yes: bool,
}

pub async fn start_and_run(config: &Config, opts: StartOptions) -> Result<()> {
    let paths = Paths::discover()?;
    crate::paths::ensure_dir(&paths.runtime_dir)?;
    crate::paths::ensure_dir(&paths.config_dir)?;

    let lockfile_path = paths.lock_file();
    let active_meeting_path = paths.active_meeting_file();
    // Acquire the lock; the guard releases it on drop. The lock is held on
    // a dedicated file (separate from `active-meeting.json`) so that atomic
    // state rewrites don't orphan it by replacing the inode.
    let _lock = acquire_lock(&lockfile_path)
        .with_context(|| "could not acquire meeting lock — is `furu start` already running?")?;

    let started = Local::now();
    let id = make_id(started, opts.title.as_deref());
    let runtime_dir = paths.runtime_meeting_dir(&id);
    crate::paths::ensure_dir(&runtime_dir)?;

    let output_root = opts
        .output_dir
        .clone()
        .unwrap_or_else(|| config.output_dir_or(&paths));
    let output_dir = output_root.join(&id);

    let notes_path = runtime_dir.join("notes.live");
    std::fs::File::create(&notes_path)
        .with_context(|| format!("could not create {}", notes_path.display()))?;

    let am = ActiveMeeting {
        id: id.clone(),
        started_at: Utc::now(),
        state: LifecycleState::Capturing,
        notes_path: notes_path.clone(),
        runtime_dir: runtime_dir.clone(),
        output_dir: output_dir.clone(),
        audio_kept: opts.keep_audio || config.keep_audio,
        attendees: opts.attendees.clone(),
    };
    write_active_meeting(&active_meeting_path, &am)?;

    let socket_path = runtime_dir.join("furugura.sock");
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("could not bind {}", socket_path.display()))?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not chmod 0600 {}", socket_path.display()))?;

    // Capture (U2)
    let wav_path = runtime_dir.join("meeting.wav");
    let capture_cfg = CaptureConfig {
        mic_source: None,
        system_source: None,
        sample_rate: config.audio_rate,
        wav_path: wav_path.clone(),
    };
    let capture_handle = start_capture(capture_cfg)
        .await
        .context("could not start audio capture (run `furu setup` to verify deps)")?;

    // Streaming whisper (U3) — best effort
    let live_jsonl_path = runtime_dir.join("transcript.live.jsonl");
    let stream_handle = match whisper_stream::start(
        whisper_stream::WhisperStreamConfig {
            model_path: PathBuf::from(&config.whisper_model_stream),
            jsonl_path: live_jsonl_path.clone(),
            sample_rate: config.audio_rate,
            binary: None,
        },
        capture_handle.broadcast.subscribe(),
    )
    .await
    {
        Ok(h) => Some(h),
        Err(e) => {
            warn!("streaming transcription disabled: {e}");
            println!("note: live transcript view disabled (whisper-stream not available)");
            None
        }
    };

    println!("furugura recording: {id}");
    println!("  notes file: {}", notes_path.display());
    println!("  socket:     {}", socket_path.display());
    println!("  output dir: {}", output_dir.display());
    println!(
        "(in another terminal: `furu mark \"...\"` to capture a note, `furu stop` to finish)",
    );

    // Server loop runs until Stop / SIGINT
    let server_state = Arc::new(Mutex::new(am.clone()));
    let server_state_for_loop = server_state.clone();
    let active_meeting_for_loop = active_meeting_path.clone();
    let notes_for_loop = notes_path.clone();
    let server_done = tokio::select! {
        r = run_server_loop(listener, server_state_for_loop, active_meeting_for_loop, notes_for_loop) => r,
        _ = tokio::signal::ctrl_c() => {
            println!();
            println!("Ctrl-C received — finalizing meeting");
            Ok(StopReason::Sigint)
        }
    };
    let _stop_reason = server_done?;

    // Halt capture + streaming
    let quality = capture_handle
        .shutdown()
        .await
        .context("capture shutdown failed")?;
    if let Some(h) = stream_handle {
        let _ = h.shutdown().await;
    }

    // Transition state
    transition_state(&active_meeting_path, LifecycleState::Finalizing)?;
    println!("finalizing — running batch transcription, diarization, and summary");

    finalize(&am, quality, config, &opts).await?;

    transition_state(&active_meeting_path, LifecycleState::Done)?;
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_dir_all(&runtime_dir);
    let _ = std::fs::remove_file(&active_meeting_path);
    let _ = std::fs::remove_file(&lockfile_path);

    println!("meeting saved to {}", output_dir.display());
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum StopReason {
    UdsStop,
    Sigint,
}

/// UDS server loop. Returns when a `Stop` request is received.
async fn run_server_loop(
    listener: UnixListener,
    state: Arc<Mutex<ActiveMeeting>>,
    active_meeting_path: PathBuf,
    notes_path: PathBuf,
) -> Result<StopReason> {
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("UDS accept failed")?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half).lines();
        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => continue,
            Err(_) => continue,
        };
        let req = match parse_request(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Err {
                    reason: format!("bad request: {e}"),
                };
                let _ = write_half
                    .write_all(format!("{}\n", render_response(&resp)).as_bytes())
                    .await;
                continue;
            }
        };

        let am_now = state.lock().await.clone();
        let resp = match req {
            Request::Mark { text, t } => {
                if am_now.state != LifecycleState::Capturing {
                    Response::Err {
                        reason: format!("meeting is {}", am_now.state.as_str()),
                    }
                } else {
                    let line = format!("[{t}] {text}\n");
                    match append_to_notes(&notes_path, &line).await {
                        Ok(()) => Response::Ok(OkPayload::MarkAck { appended: line.trim_end().to_string() }),
                        Err(e) => Response::Err { reason: format!("append failed: {e}") },
                    }
                }
            }
            Request::Status => {
                let elapsed = (Utc::now() - am_now.started_at).num_seconds();
                Response::Ok(OkPayload::Status {
                    state: am_now.state.as_str().to_string(),
                    elapsed_seconds: elapsed,
                })
            }
            Request::Stop => {
                // Reply *before* mutating, then break the loop.
                let resp = Response::Ok(OkPayload::StopAck {
                    state: LifecycleState::Finalizing.as_str().to_string(),
                });
                let _ = write_half
                    .write_all(format!("{}\n", render_response(&resp)).as_bytes())
                    .await;
                let _ = write_half.shutdown().await;
                // Persist to lockfile so other readers see the new state.
                if let Err(e) = transition_state(&active_meeting_path, LifecycleState::Finalizing) {
                    warn!("failed to write finalizing state: {e}");
                }
                state.lock().await.state = LifecycleState::Finalizing;
                return Ok(StopReason::UdsStop);
            }
        };
        let _ = write_half
            .write_all(format!("{}\n", render_response(&resp)).as_bytes())
            .await;
        let _ = write_half.shutdown().await;
    }
}

async fn append_to_notes(path: &Path, line: &str) -> Result<()> {
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    f.write_all(line.as_bytes()).await?;
    f.sync_data().await?;
    Ok(())
}

/// Run the post-stop pipeline. Each step degrades gracefully when its
/// external binary is missing; the markdown file is always produced.
pub async fn finalize(
    am: &ActiveMeeting,
    capture_quality: CaptureQuality,
    config: &Config,
    opts: &StartOptions,
) -> Result<()> {
    let wav_path = am.runtime_dir.join("meeting.wav");

    // Batch transcription
    let mut segments: Vec<LiveSegment> = match run_batch_transcription(&wav_path, config).await {
        Ok(segs) => segs,
        Err(e) => {
            warn!("batch transcription failed: {e}");
            println!("warning: batch transcription failed ({e}) — see frontmatter for engine info");
            // Fall back to whatever the streaming pass captured.
            let live_jsonl = am.runtime_dir.join("transcript.live.jsonl");
            jsonl::read_segments(&live_jsonl).unwrap_or_default()
        }
    };

    // Diarization
    let (rttm_segments, rttm_text) =
        match run_diarization(&wav_path, &am.id, config).await {
            Ok((segs, text)) => (segs, text),
            Err(e) => {
                warn!("diarization skipped: {e}");
                println!("warning: diarization skipped ({e})");
                (Vec::new(), String::new())
            }
        };
    if !rttm_segments.is_empty() {
        segments = merge_speaker_labels(&segments, &rttm_segments);
    }

    // Summary
    let (summary_block, audit) = match run_summary(&segments, &am.notes_path, config, opts).await {
        Ok(t) => t,
        Err(e) => {
            warn!("summary skipped: {e}");
            println!("warning: summary skipped ({e})");
            (
                SummaryBlock::default(),
                SummaryAudit {
                    provider: provider_label(config),
                    model: model_label(config),
                    data_egressed: DataEgressed::None,
                },
            )
        }
    };

    // Assemble markdown
    let notes = std::fs::read_to_string(&am.notes_path).unwrap_or_default();
    let title = derive_title(&am.id, opts.title.as_deref());
    let body = MeetingBody {
        title,
        notes,
        summary: summary_block,
        transcript: segments.clone(),
    };

    let frontmatter = MeetingFrontmatter {
        id: am.id.clone(),
        start_time: am.started_at.with_timezone(&Local),
        end_time: Local::now(),
        attendees: am.attendees.clone(),
        audio_retained: am.audio_kept,
        capture_quality,
        transcription_engine: format!("whisper.cpp:{}", config.whisper_model_batch),
        diarization_model: if rttm_segments.is_empty() {
            "none".to_string()
        } else {
            "pyannote-3.1".to_string()
        },
        summary_audit: audit,
        tags: vec!["meeting".to_string()],
    };
    let md = assemble(&frontmatter, &body);
    crate::paths::ensure_dir(&am.output_dir)?;
    write_meeting_md(&am.output_dir, &md)?;

    // Persist transcript.jsonl
    let tj_path = am.output_dir.join("transcript.jsonl");
    if let Err(e) = jsonl::write_segments(&tj_path, &segments) {
        warn!("could not write transcript.jsonl: {e}");
    }

    // Persist RTTM sidecar (only if we have one)
    if !rttm_text.is_empty() {
        let rttm_path = pyannote::rttm_sidecar_path(&am.output_dir, &am.id);
        if let Err(e) = std::fs::write(&rttm_path, rttm_text) {
            warn!("could not write RTTM sidecar: {e}");
        }
    }

    // Audio handling
    handle_audio(&wav_path, &am.output_dir, am.audio_kept).await?;

    Ok(())
}

async fn run_batch_transcription(
    wav: &Path,
    config: &Config,
) -> Result<Vec<LiveSegment>> {
    let stem = wav.with_extension("whisper");
    let cfg = whisper_batch::WhisperBatchConfig {
        model_path: PathBuf::from(&config.whisper_model_batch),
        wav_path: wav.to_path_buf(),
        output_stem: stem,
        threads: None,
        binary: None,
    };
    let out = whisper_batch::run(cfg).await?;
    Ok(out.segments)
}

async fn run_diarization(
    wav: &Path,
    file_id: &str,
    config: &Config,
) -> Result<(Vec<RttmSegment>, String)> {
    let paths = Paths::discover()?;
    let token_path = config.hf_token_path_or(&paths);
    let token = std::fs::read_to_string(&token_path)
        .with_context(|| format!("HF token missing at {}", token_path.display()))?;
    let cfg = pyannote::DiarizeConfig {
        wav_path: wav.to_path_buf(),
        hf_token: token.trim().to_string(),
        binary: None,
        file_id: file_id.to_string(),
    };
    let result = pyannote::run(cfg).await?;
    Ok((result.segments, result.rttm_text))
}

async fn run_summary(
    segments: &[LiveSegment],
    notes_path: &Path,
    config: &Config,
    opts: &StartOptions,
) -> Result<(SummaryBlock, SummaryAudit)> {
    let notes = std::fs::read_to_string(notes_path).unwrap_or_default();
    let prompt = prompt::assemble_prompt(prompt::PromptInputs {
        user_notes: &notes,
        transcript: segments,
    });

    let provider_kind = effective_provider(config, opts);
    let consent = evaluate_consent(ConsentInputs {
        provider: provider_kind,
        attendees: &opts.attendees,
        yes_flag: opts.yes,
        attendee_consent_flag: opts.i_have_consent,
    });
    match consent {
        ConsentDecision::RefuseAttendees => {
            return Err(anyhow!(
                "refusing cloud summary: meeting has named attendees who have not consented \
                 to third-party processing. Pass --i-have-consent to override.",
            ));
        }
        ConsentDecision::NeedsInteractivePrompt => {
            let model = opts.cloud_model.as_deref().unwrap_or(&config.summary_model);
            println!(
                "\nNotice: transcript will be sent to Anthropic API ({}). \
                 Press Enter to continue or Ctrl-C to abort.",
                model,
            );
            use std::io::BufRead;
            let stdin = std::io::stdin();
            let mut lock = stdin.lock();
            let mut buf = String::new();
            lock.read_line(&mut buf)?;
        }
        ConsentDecision::Proceed => {}
    }

    match provider_kind {
        ProviderKind::Local => {
            let endpoint = ollama::resolve_endpoint(
                std::env::var("OLLAMA_HOST").ok().as_deref(),
                config.summary_endpoint.as_deref(),
            );
            if !ollama::endpoint_is_loopback(&endpoint) {
                println!(
                    "Warning: Ollama endpoint is not local ({endpoint}); transcript content will leave this machine",
                );
            }
            let resp = ollama::generate(ollama::OllamaRequest {
                endpoint: endpoint.clone(),
                model: config.summary_model.clone(),
                prompt,
                num_ctx: config.num_ctx,
                temperature: 0.2,
                keep_alive: "30m".to_string(),
            })
            .await?;
            let block = parse_summary(&resp);
            let audit = SummaryAudit {
                provider: "local".to_string(),
                model: format!("ollama:{}", config.summary_model),
                data_egressed: if ollama::endpoint_is_loopback(&endpoint) {
                    DataEgressed::None
                } else {
                    DataEgressed::FullTranscript
                },
            };
            Ok((block, audit))
        }
        ProviderKind::CloudAnthropic => {
            let model = opts.cloud_model.clone().unwrap_or_else(|| config.summary_model.clone());
            let paths = Paths::discover()?;
            let token = anthropic::load_token(&paths.anthropic_token_file())?;
            let resp = anthropic::messages(anthropic::AnthropicRequest {
                api_token: token,
                model: model.clone(),
                system: prompt::SYSTEM_INSTRUCTIONS.to_string(),
                user: prompt,
                max_tokens: 4096,
            })
            .await?;
            let block = parse_summary(&resp);
            let audit = SummaryAudit {
                provider: "cloud:anthropic".to_string(),
                model: format!("anthropic:{model}"),
                data_egressed: DataEgressed::FullTranscript,
            };
            Ok((block, audit))
        }
    }
}

fn effective_provider(config: &Config, opts: &StartOptions) -> ProviderKind {
    if opts.cloud_model.is_some() {
        return ProviderKind::CloudAnthropic;
    }
    match config.summary_provider {
        SummaryProvider::Local => ProviderKind::Local,
        SummaryProvider::CloudAnthropic => ProviderKind::CloudAnthropic,
    }
}

fn provider_label(config: &Config) -> String {
    match config.summary_provider {
        SummaryProvider::Local => "local".to_string(),
        SummaryProvider::CloudAnthropic => "cloud:anthropic".to_string(),
    }
}

fn model_label(config: &Config) -> String {
    match config.summary_provider {
        SummaryProvider::Local => format!("ollama:{}", config.summary_model),
        SummaryProvider::CloudAnthropic => format!("anthropic:{}", config.summary_model),
    }
}

fn derive_title(id: &str, override_title: Option<&str>) -> String {
    if let Some(t) = override_title {
        return t.to_string();
    }
    // Strip the date+time prefix: `2026-05-02-1430-rest-of-slug`
    // or `2026-05-02-1430` (no slug).
    let parts: Vec<&str> = id.splitn(5, '-').collect();
    if parts.len() == 5 {
        humanize_slug(parts[4])
    } else {
        id.to_string()
    }
}

fn humanize_slug(slug: &str) -> String {
    if slug.is_empty() {
        return "Meeting".to_string();
    }
    let parts: Vec<String> = slug
        .split('-')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    parts.join(" ")
}

async fn handle_audio(
    wav: &Path,
    output_dir: &Path,
    keep: bool,
) -> Result<()> {
    if !wav.exists() {
        return Ok(());
    }
    if !keep {
        let _ = std::fs::remove_file(wav);
        return Ok(());
    }
    let opus_path = output_dir.join("audio.opus");
    let status = tokio::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-i").arg(wav)
        .arg("-c:a").arg("libopus")
        .arg(&opus_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    match status {
        Ok(s) if s.success() => {
            let _ = std::fs::remove_file(wav);
        }
        Ok(s) => {
            warn!("ffmpeg exited {s} — keeping raw WAV");
            let kept = output_dir.join("meeting.wav");
            let _ = std::fs::rename(wav, kept);
        }
        Err(e) => {
            warn!("ffmpeg invocation failed: {e} — keeping raw WAV");
            let kept = output_dir.join("meeting.wav");
            let _ = std::fs::rename(wav, kept);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_title_humanizes_slug() {
        assert_eq!(
            derive_title("2026-05-02-1430-team-standup", None),
            "Team Standup",
        );
        assert_eq!(
            derive_title("2026-05-02-1430-q3-planning-sales", None),
            "Q3 Planning Sales",
        );
    }

    #[test]
    fn derive_title_falls_back_to_id_when_no_slug() {
        // No slug → can't split into 5 parts → fallback to id.
        assert_eq!(
            derive_title("2026-05-02-1430", None),
            "2026-05-02-1430",
        );
    }

    #[test]
    fn derive_title_uses_override() {
        assert_eq!(
            derive_title("2026-05-02-1430-team-standup", Some("Special Title")),
            "Special Title",
        );
    }

    #[test]
    fn humanize_slug_basics() {
        assert_eq!(humanize_slug(""), "Meeting");
        assert_eq!(humanize_slug("standup"), "Standup");
        assert_eq!(humanize_slug("team-standup"), "Team Standup");
    }

    #[test]
    fn provider_label_matches_config() {
        let local = Config {
            summary_provider: SummaryProvider::Local,
            ..Config::default()
        };
        assert_eq!(provider_label(&local), "local");
        let cloud = Config {
            summary_provider: SummaryProvider::CloudAnthropic,
            ..Config::default()
        };
        assert_eq!(provider_label(&cloud), "cloud:anthropic");
    }

    #[test]
    fn effective_provider_cloud_flag_forces_cloud() {
        let c = Config::default();
        let opts = StartOptions {
            keep_audio: false,
            output_dir: None,
            title: None,
            attendees: vec![],
            cloud_model: Some("claude-opus-4-7".into()),
            i_have_consent: false,
            yes: false,
        };
        assert_eq!(effective_provider(&c, &opts), ProviderKind::CloudAnthropic);
    }
}

