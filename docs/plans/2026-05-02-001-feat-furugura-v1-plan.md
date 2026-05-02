---
title: "feat: Furugura v1 — CLI meeting capture and summary"
type: feat
status: active
date: 2026-05-02
origin: docs/brainstorms/2026-05-02-furugura-v1-requirements.md
deepened: 2026-05-02
---

# feat: Furugura v1 — CLI meeting capture and summary

## Summary

Furugura v1 ships as a Rust single-binary that subprocesses two `pw-record` instances for mic + system audio capture (always written to a tmpfs WAV), drives `whisper.cpp` with Vulkan acceleration for transcription (matching Omarchy's voxtype convention), runs `pyannote.audio` standalone for diarization at finalize time, calls Ollama over HTTP for local summarization, and writes a per-meeting markdown file under `~/Meetings/<id>/`. A Ratatui TUI consumes a live-streaming `transcript.live.jsonl` produced during the meeting; a `flock`-locked `active-meeting.json` plus a Unix-domain socket coordinate `furu mark` and `furu stop` from separate terminals.

---

## Problem Frame

Origin doc carries the full pain narrative (Linux user on Omarchy must context-switch to macOS to run Granola). Plan-side: the implementation must work on a fresh Omarchy box (PipeWire 1.6.4, Intel Arc / AMD / NVIDIA GPUs, no whisper / pyannote / ollama installed by default) without inventing a daemon, GUI, or proprietary format. Vendor portability matters: WhisperX-on-CUDA only would have excluded the user's actual hardware (Intel Arc B390 iGPU). Whisper.cpp + Vulkan covers the full GPU vendor matrix and matches the convention Omarchy already established with `voxtype-bin`. (see origin: `docs/brainstorms/2026-05-02-furugura-v1-requirements.md`)

---

## Requirements

R-IDs trace 1:1 to origin requirements; rephrased here for plan-perspective.

- R1. Capture mic and system audio simultaneously on PipeWire-based Linux. Omarchy / Hyprland is the dev target; other PipeWire-based distros are best-effort. Origin: A1, F1.
- R2. Begin via explicit `furu start`, end via `furu stop`. No auto-trigger in v1. Origin: F1.
- R3. Hold raw audio in volatile (tmpfs / RAM) storage; discard at finalization by default. Opt-in retention via `--keep-audio` moves the WAV to durable storage at finalize. Origin: AE1, AE2.
- R4. Transcription uses `whisper.cpp` with Vulkan acceleration; speaker diarization runs `pyannote.audio` on the finalized audio at `furu stop`. Output: a transcript with timestamps, words, and per-speaker labels. Origin: AE3.
- R5. Post-meeting summary generated locally by default (Ollama, on-loopback only); cloud LLM APIs (Anthropic) are opt-in per meeting and gated by an explicit consent prompt. Origin: AE6.
- R6. Speaker labels in v1 output are generic identifiers (`SPEAKER_00`, `SPEAKER_01`, …) the user can rename in the output file. No persistent voiceprint store in v1.
- R7. `furu live` is a TUI scrollable line-by-line, with substring search. Reads from `transcript.live.jsonl` produced by a streaming whisper.cpp pass during the meeting. Origin: F2, AE4.
- R8. `furu mark "<text>"` captures a timestamped marker that lands in the final markdown's notes section verbatim. Origin: AE5.
- R9. Output for each meeting is a markdown file with YAML frontmatter, written under a configurable directory.
- R10. Frontmatter records date, duration, attendees (best-effort, may be empty), audio retention status, transcription engine, summary model, summary provider (local/cloud), and capture quality (degraded vs. clean). Origin context expanded for audit auditability.
- R11. Body has three distinct H2 sections in order: user manual notes (verbatim, timestamped), AI summary (with three H3 subsections — Decisions, Action items, Key points), full transcript with timestamps and speaker labels.
- R12. `furu list` and `furu edit <id>` discover past meetings and open them in `$EDITOR`. Origin: F3.
- R13. Furugura v1 ships as a single binary. No daemon. `furud` is deferred to v2.

**Origin actors:** A1 (Linux user), A2 (Downstream consumer — Talos / Obsidian / agentic ingestor).

**Origin flows:** F1 (capture end-to-end), F2 (catch up after distraction via live streaming preview), F3 (manual review), F4 (downstream hand-off).

**Origin acceptance examples:** AE1 (covers R3 default), AE2 (covers R3 opt-in), AE3 (covers R4, R6), AE4 (covers R7), AE5 (covers R8, R11), AE6 (covers R5).

---

## Scope Boundaries

### Deferred for later

Carried from origin (product/version sequencing).

- Persistent voiceprint store keyed on email — v1.5
- Calendar polling for auto-start (CalDAV / Google Calendar) — v1.6
- `furud` daemon (systemd `--user` service) — v2
- Standalone `wl-meetcap` upstream contribution — v2
- MCP server inside Furugura — v2
- Live AI summary during the meeting — deferred until v2 evaluates need
- Numbers / names / acronyms vocabulary file with whisper `initial_prompt` injection — v1.5
- Crash-safe append-only WAL chunked Opus capture — v2
- Stenographer-style two-engine verification — v2

### Outside this product's identity

Carried from origin (positioning rejection).

- Vector search / personal-corpus query (Talos + QMD owns this)
- Spaced-repetition pings on commitments
- Hash-chained ELN-style append-only integrity log
- Hardware foot-pedal / macropad UX
- Host-side capture appliance (Pi / NUC SFU)
- Multi-user / shared-transcript collaboration
- macOS / Windows ports — Linux-only
- Browser extension as primary UX surface
- GUI shell (Tauri / Electron / Qt)
- OMF as a standalone JSON-LD spec ahead of any clients
- Editor-embedded live notes-spine (typed notes merge with transcript in real time)

### Deferred to Follow-Up Work

Plan-local — implementation work intentionally split out of this plan.

- Hot-loaded pyannote Python sidecar to amortize cold-start: v2 optimization once cold-start latency surfaces as a real complaint.
- Native `pipewire` Rust crate integration (vs. subprocessing `pw-record`): defer until the daemon ships.
- mlock'd audio buffers: v2 privacy hardening (current swap-exposure mitigation: tmpfs + startup `/proc/swaps` advisory).
- FAISS / hnswlib voiceprint matching: overkill at <50 voiceprints; numpy brute-force is microseconds. Revisit only past ~1000 voices.
- DBus session bus integration / systemd-run mode: premature daemon surface area for v1.
- WhisperX with bundled diarization: defer until a Vulkan/ROCm/oneAPI compute backend lands in CTranslate2 (then re-evaluate vs. whisper.cpp + standalone pyannote).
- UDS `notify_mark` for live-view marker rendering: defer to v1.5 if UX feedback requests it; v1's `furu live` does not render `furu mark` events inline.

---

## Context & Research

### Relevant Code and Patterns

- The user's box is Omarchy (Arch + Hyprland). Omarchy's own `voxtype-bin` (AUR) is whisper.cpp + Vulkan + systemd `--user` service — Furugura matches the conventions: AUR `-bin` package eventually, `~/.config/furugura/config.toml`, mode-0600 for sensitive files, Vulkan-accelerated whisper.cpp as the transcription engine.
- Omarchy ships PipeWire 1.6.4 + portaudio. No whisper / pyannote / ollama installed by default; `furu setup` onboards them.
- The user runs Talos MCP (Ruby + SQLite + markdown/YAML-frontmatter conventions) as one downstream consumer. Furugura's markdown shape works for Talos's auto-extract behaviors but does not bake Talos-specific assumptions in load-bearing form.

### Institutional Learnings

- No `docs/solutions/` exists in `~/Projects/furugura/` (greenfield).

### External References

- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) — C/C++ Whisper port with Vulkan, CUDA, Metal, OpenCL, OpenBLAS backends. Stream example supports near-real-time transcription with VAD and overlap windows. v1 transcription engine.
- [pyannote/speaker-diarization-3.1](https://huggingface.co/pyannote/speaker-diarization-3.1) — gated; HF token required at first run. Runs on CUDA when available, falls back to CPU; for once-per-meeting batch use, CPU pyannote is acceptable.
- [pyannote/embedding](https://huggingface.co/pyannote/embedding) — 512-dim x-vector embeddings for v1.5 voiceprint enrollment. Numpy brute-force cosine over <50 voices is microseconds.
- [Granary](https://github.com/wassimk/granary) — a third-party Granola-export tool whose markdown turn format (`**SPEAKER_NN:** text`) Furugura adopts for transcript blocks so existing OSS parsers (Granary's, Granola CLI tools) work on Furugura's output.
- [Ollama API docs](https://github.com/ollama/ollama/blob/main/docs/api.md) — `/api/generate` + `/api/chat`, `keep_alive`, `num_ctx`. Respects `OLLAMA_HOST` env var, which Furugura must check at runtime to enforce its local-by-default privacy posture.
- [Gemma 3 (Ollama)](https://ollama.com/library/gemma3) — 128K context, strong instruction-following.
- [Ratatui v0.30 highlights](https://ratatui.rs/highlights/v030/) — `insert_before()` is a near-perfect fit for live-transcript scrollback.
- [pw-record / pw-cat](https://docs.pipewire.org/page_man_pw-record_1.html) — workhorse capture command for the dual-subprocess pattern.
- [Arch Wiki — XDG Base Directory](https://wiki.archlinux.org/title/XDG_Base_Directory) — `$XDG_RUNTIME_DIR` is tmpfs-backed, mode 0700, the right home for ephemeral audio. Note: tmpfs pages can swap to disk on systems with active swap unless mlock'd; v1 mitigation is a startup advisory if `/proc/swaps` is non-empty.
- [Granola privacy policy](https://docs.granola.ai/help-center/policies/privacy-policy) — "audio not retained once transcription is created" — Furugura's tmpfs+unlink default matches this stance for the on-disk discard portion.

---

## Key Technical Decisions

- **Language: Rust.** Best PipeWire binding ecosystem, smallest static-musl binary, idiomatic AUR packaging, Ratatui v0.30 fits the live-transcript pattern.
- **Transcription engine: whisper.cpp with Vulkan acceleration.** Vendor-portable across NVIDIA / AMD / Intel; matches Omarchy's `voxtype-bin` convention; supports a streaming mode (`whisper-stream` / `whisper-cli --stream`) used during the meeting AND a batch mode used at finalization for higher accuracy. Replaces the original WhisperX-CTranslate2 choice, which required CUDA and would have excluded the user's Intel Arc box.
- **Diarization: pyannote.audio standalone, run at `furu stop`.** Falls back to CPU when CUDA absent. Requires a HuggingFace token (still gated as of 2026-05); `furu setup` onboards it. Token is passed via `HF_TOKEN` environment variable on subprocess invocation, not on the command line — this avoids `/proc/<pid>/cmdline` leakage. Token storage: `~/.config/furugura/hf_token` mode 0600.
- **Audio capture: dual `pw-record` subprocesses, 48 kHz mono each, interleaved to stereo in-process, always written to a tmpfs WAV** at `$XDG_RUNTIME_DIR/furugura/<id>/meeting.wav` regardless of `--keep-audio`. The WAV is the bridge to whisper.cpp + pyannote (file-path consumers), the crash-recovery target, and (when retained) the v1.5 voiceprint-enrollment source. `--keep-audio` only controls preserve-vs-unlink at finalize: opt-in moves the WAV (re-encoded to Opus via `ffmpeg`) to `~/Meetings/<id>/audio.opus`; default unlinks.
- **System-audio source resolution: `pactl list short sources` filtered for `.monitor` suffix**, picking the monitor of the default sink (queryable via `pactl info` or PipeWire's `default.audio.sink` metadata). The original plan's `pw-cli list-objects Node` doesn't list monitor sources. Document the multi-sink case (capture monitor of default sink only).
- **Channel-split prior is a hint, not a guarantee.** At `furu start`, detect headphone/Bluetooth output presence; warn if none (laptop-speaker bleed risk: mic captures both user and remote audio, breaking the "left=me, right=them" prior). On `pw-record` node disappearance mid-meeting (BT profile switch, USB hot-plug), attempt re-resolution before failing. Record `capture_quality: degraded` in frontmatter on drift > threshold or device-switch events; surface in `furu stop` output.
- **Whisper model: `large-v3` GGUF for batch finalization** (whisper.cpp accepts GGUF; configurable via `--model`); **`base.en` GGUF for the live streaming preview** (low-latency, much lower accuracy — overwritten by the batch pass at finalize).
- **LLM: Ollama with Gemma 3 4B default**, `num_ctx: 16384`, `keep_alive: "30m"`. Configurable via `--model`. Fallback ladder: Gemma 3 1B → Qwen 3 1.7B for low-VRAM users. **Loopback enforcement**: Furugura resolves the effective Ollama endpoint (`OLLAMA_HOST` env, then config, then `localhost:11434`); if the resolved host is not loopback, a warning surfaces (`Warning: Ollama endpoint is not local; transcript content will leave this machine`). The "local-by-default" privacy guarantee is enforced at runtime, not assumed.
- **Cloud LLM consent gate.** When `--cloud-model` is set or `summary_model` in config points at a cloud provider, `furu stop` prints a single-line consent prompt before invoking summarization (`Notice: transcript will be sent to <provider> API (<model>). Press Enter to continue or Ctrl-C to abort`). A `--yes` flag skips for scripted use. When `--attendees` is non-empty, the cloud path is refused unless `--i-have-consent` is also passed (third-party data egress requires explicit acknowledgment). Frontmatter records `summary_provider: local|cloud:<provider>` and `data_egressed: none|full_transcript` for downstream audit.
- **Marker IPC: hybrid lockfile + UDS.** `flock`-locked `$XDG_RUNTIME_DIR/furugura/active-meeting.json` carries the in-progress notes-file path. `furu mark` canonicalizes the resolved path and asserts it is a strict prefix-child of `$XDG_RUNTIME_DIR/furugura/`; rejects on path escape. UDS at `furugura.sock` is the primary path for marker delivery (orchestrator owns the notes-file write, sequencing relative to `furu stop`); direct file append is the fallback when UDS is unreachable. The orchestrator refuses new UDS marks once finalization begins. UDS `notify_mark` for live-view marker rendering is deferred to v1.5.
- **TUI: Ratatui** with `insert_before()` for stream-friendly append; reads `transcript.live.jsonl` produced by a streaming `whisper-stream` subprocess started with `furu start`.
- **Lifecycle state machine: `capturing` → `finalizing` → `done`.** `furu stop` returns immediately to the caller after sending the stop signal over UDS; the orchestrator transitions to `finalizing` (releases the recording lock; runs whisper.cpp batch + pyannote + Ollama + markdown writer in background) and then `done` (releases the lockfile, removes the socket). A second `furu stop` invocation on a `finalizing` meeting prints progress; `furu start` on a `finalizing` lockfile prints "previous meeting still finalizing" and exits. Crash recovery is not interactive: a stale lockfile triggers `furu cleanup`-style messaging — `Stale meeting state found for <id> — run 'furu cleanup' to discard or 'furu finalize <id>' to retry the post-stop pipeline if audio was retained`. `furu cleanup` is a one-liner that unlinks the runtime dir and socket.
- **Markdown schema: three top-level H2s (`## Notes`, `## Summary`, `## Transcript`) with the `## Summary` H2 containing three H3 subsections (`### Decisions`, `### Action items`, `### Key points`)** + YAML frontmatter (snake_case, ISO 8601, `furugura_version: 1`). Decisions use `**Decision:**` prefix; action items use GitHub task-list syntax (`- [ ] Name — task (due YYYY-MM-DD)`); transcript turns use `**SPEAKER_NN:**` (matches Granary's exporter).
- **Per-meeting directory layout: `~/Meetings/<id>/`.** Contains `meeting.md`, `transcript.live.jsonl` (streaming preview during the meeting; persisted), `transcript.jsonl` (authoritative post-`furu stop` output), `<id>.diarization.rttm`. ID is the filename stem (`YYYY-MM-DD-HHMM-<slug>`). No state DB — `furu list` is a directory glob + frontmatter parse.
- **RTTM sidecar persisted in v1** for v1.5 retroactive voiceprint enrollment, **scoped to `--keep-audio` meetings only**: voiceprint enrollment requires the audio bytes pyannote/embedding consumes, which only exist on retained meetings. Default-discard meetings have transcripts and rename pairs but no enrollment path.
- **Audio-discard privacy framing: tmpfs + unlink prevents recovery from persistent block storage; data may still reach swap on systems with active swap.** v1 mitigation: at startup, check `/proc/swaps`; if non-empty, emit a one-time advisory ("audio held in tmpfs may page to swap before deletion; v2 will mlock buffers"). v2 hardens with `mlock`. The privacy story is honest about the boundary of the guarantee.

---

## Open Questions

### Resolved During Planning

- Language choice: **Rust**.
- Transcription engine: **whisper.cpp + Vulkan** (replaces WhisperX/CTranslate2 — required CUDA, would exclude user's Intel Arc).
- Diarization: **pyannote.audio standalone** at finalize.
- LLM default: **Gemma 3 4B**.
- TUI mechanism: **Ratatui** primary, JSONL fallback for power users.
- Marker IPC: **lockfile + UDS hybrid** (UDS primary path).
- v1 → v1.5 voiceprint migration: **persist RTTM sidecar in v1; v1.5 walks rename pairs and matches against retained audio (--keep-audio meetings only)**.
- v1 attendee detection: **empty by default + optional `--attendees` flag**.
- `furu stop` UX: **returns immediately; finalization runs in background; lockfile state machine reports progress**.
- Live transcript view source: **streaming `whisper-stream` subprocess writes `transcript.live.jsonl` during the meeting**; authoritative pass at finalize overwrites with `transcript.jsonl`.
- HF token passing: **subprocess env (`HF_TOKEN`), not CLI flag** (cmdline leak prevention).

### Deferred to Implementation

- Exact pw-record buffering / sample-rate alignment between subprocesses — verify no clock drift on a 30-min recording before declaring U2 done.
- whisper.cpp CLI flag stability — verify the streaming and batch flag surfaces are stable enough across whisper.cpp releases for a single subprocess wrapper to handle both.
- Ollama prompt template tuning — start from a reasonable baseline; iterate based on actual transcript output during U9 development.
- Ratatui's `insert_before()` integration with JSONL tailing — discoverable in U7.
- Best Vulkan-compatible Whisper GGUF model size for the batch pass on the user's iGPU — likely `large-v3` Q5_K, but worth a side-by-side benchmark.

### Deferred from Doc Review

- Markdown schema universality vs Talos-shaped assumptions: the schema chose `**Decision:**` prefix and three-H3-subsection split partly because Talos extracts cleanly from those shapes. A second downstream consumer (Obsidian Dataview, agentic ingestor with different conventions) may motivate splitting "universal" vs "Talos-specific" fields in the README schema spec. Resolve when a second consumer surfaces.
- v1.5 voiceprint shipping cadence: success criterion is "user stops switching to Mac from week 2." If recurring meetings re-tax the user with renames every week, v1.5 may need to ship within ~4 weeks of v1, not as a separate phase. Empirical question — depends on real v1 usage.
- `furugura_version` migration story: forward-compat default in v1 ("implementations tolerate unknown fields"); concrete v1.5 schema migration plan written when v1.5 fields are designed. Currently no consumer branches on `furugura_version`.

---

## Output Structure

```
furugura/
├── Cargo.toml
├── Cargo.lock
├── README.md                          # documents the markdown schema (the v1 "format spec")
├── LICENSE
├── .gitignore
├── docs/
│   ├── brainstorms/
│   │   └── 2026-05-02-furugura-v1-requirements.md
│   └── plans/
│       └── 2026-05-02-001-feat-furugura-v1-plan.md
├── packaging/
│   └── PKGBUILD                       # AUR -bin package (deferred polish)
└── src/
    ├── main.rs                        # CLI entry (clap)
    ├── lib.rs                         # public surface
    ├── cli/
    │   ├── mod.rs
    │   ├── start.rs                   # `furu start`
    │   ├── stop.rs                    # `furu stop`
    │   ├── mark.rs                    # `furu mark`
    │   ├── list.rs                    # `furu list`
    │   ├── edit.rs                    # `furu edit`
    │   ├── live.rs                    # `furu live`
    │   ├── cleanup.rs                 # `furu cleanup`
    │   ├── finalize.rs                # `furu finalize <id>` — retry post-stop pipeline
    │   └── setup.rs                   # `furu setup`
    ├── config.rs                      # ~/.config/furugura/config.toml
    ├── paths.rs                       # XDG dirs
    ├── audio/
    │   ├── mod.rs
    │   ├── capture.rs                 # dual pw-record subprocess manager + always-write WAV
    │   ├── interleave.rs              # mic-mono + sys-mono → stereo s16
    │   └── source_resolve.rs          # pactl-based monitor source resolution
    ├── transcribe/
    │   ├── mod.rs
    │   ├── whisper_stream.rs          # streaming whisper.cpp subprocess (live preview)
    │   ├── whisper_batch.rs           # batch whisper.cpp subprocess (finalize)
    │   ├── pyannote.rs                # standalone diarization subprocess
    │   ├── jsonl.rs                   # transcript.jsonl writer/reader
    │   └── rttm.rs                    # RTTM sidecar writer
    ├── lifecycle/
    │   ├── mod.rs
    │   ├── lockfile.rs                # flock + active-meeting.json + state machine
    │   └── uds.rs                     # Unix-domain-socket server
    ├── live/
    │   └── tui.rs                     # Ratatui app (search inlined as private state)
    ├── summarize/
    │   ├── mod.rs
    │   ├── ollama.rs                  # reqwest HTTP client + loopback check
    │   ├── anthropic.rs               # cloud opt-in client + consent gate
    │   └── prompt.rs                  # system + user prompt templates
    └── output/
        └── markdown.rs                # final meeting.md assembler (frontmatter writer inlined)
```

The implementer may adjust this layout if a better organization surfaces. Per-unit `**Files:**` sections remain authoritative.

---

## High-Level Technical Design

> *Directional guidance for review, not implementation specification.*

```mermaid
flowchart LR
  subgraph "$XDG_RUNTIME_DIR/furugura/<id>/"
    LF[active-meeting.json + flock<br/>state: capturing | finalizing | done]
    SOCK[furugura.sock]
    WAV[meeting.wav<br/>tmpfs, always written]
    NOTES[notes.live<br/>marker appends]
    JSONL_LIVE[transcript.live.jsonl<br/>streaming preview]
  end

  subgraph PipeWire
    MIC[mic source]
    SYS[default sink monitor]
  end

  CLI_START[furu start] -->|spawn| LF
  CLI_START -->|bind| SOCK
  CLI_START -->|create| NOTES
  CLI_START -->|spawn| CAP

  CAP[audio::capture<br/>2× pw-record] --> MIC
  CAP --> SYS
  CAP -->|interleaved s16 stereo| WAV
  CAP -->|tail audio| WSTREAM

  WSTREAM[whisper-stream<br/>base.en GGUF<br/>Vulkan]
  WSTREAM -->|low-latency segments| JSONL_LIVE

  CLI_MARK[furu mark text] -->|primary path| SOCK
  SOCK -->|orchestrator append| NOTES
  CLI_MARK -.fallback.-> NOTES

  CLI_LIVE[furu live] -->|tail -f| JSONL_LIVE

  CLI_STOP[furu stop] -->|signal stop| SOCK
  SOCK -->|state: finalizing| FIN

  FIN[finalize pipeline<br/>background]
  FIN -->|consume| WAV
  FIN --> WBATCH[whisper.cpp batch<br/>large-v3 GGUF<br/>Vulkan]
  WBATCH --> PYAN[pyannote diarize]
  PYAN --> JSONL[transcript.jsonl]
  PYAN --> RTTM[<id>.diarization.rttm]

  FIN --> SUM[summarize::ollama<br/>loopback check<br/>cloud consent gate]
  SUM --> MD[~/Meetings/<id>/meeting.md]
  PYAN --> MD
  NOTES --> MD
  RTTM -.--> RTTM_OUT[~/Meetings/<id>/<id>.diarization.rttm]
  WAV -.--> AUD_OUT[~/Meetings/<id>/audio.opus<br/>--keep-audio only]
```

---

## Implementation Units

### Phase 1 — Foundation

- U1. **Project bootstrap**

**Goal:** Cargo workspace, module skeleton, config schema, XDG path resolution, README + LICENSE + .gitignore.

**Requirements:** R13.

**Dependencies:** None.

**Files:**
- Create: `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/lib.rs`, `src/cli/mod.rs`, `src/config.rs`, `src/paths.rs`, `README.md`, `LICENSE`, `.gitignore`
- Test: `tests/config_test.rs`

**Approach:**
- `clap` derive for the CLI surface (subcommands: `start`, `stop`, `mark`, `list`, `edit`, `live`, `setup`, `cleanup`, `finalize`).
- `directories` crate for XDG paths.
- Config schema: `whisper_model_batch`, `whisper_model_stream`, `summary_model`, `summary_provider`, `num_ctx`, `output_dir`, `hf_token_path`, `audio_rate`, `keep_audio` defaults. Loaded via `serde` from `~/.config/furugura/config.toml` with environment-variable override (`FURU_*`).
- License: MIT.

**Patterns to follow:** clap derive idioms, standard `lib.rs`/`main.rs` split.

**Test scenarios:**
- *Happy path:* Given a valid `config.toml`, when the loader runs, then `Config` deserializes with documented defaults applied.
- *Edge case:* Given the config file does not exist, when the loader runs, then it returns the all-default `Config` without error.
- *Error path:* Given invalid TOML, when the loader runs, then it returns an error naming the invalid line.

**Verification:** `cargo build` produces `furu`; `furu --help` lists all subcommands; `cargo test` passes config tests.

---

### Phase 2 — Capture pipeline

- U2. **Audio capture (dual pw-record subprocesses + always-write tmpfs WAV)**

**Goal:** Spawn two `pw-record` subprocesses (mic + monitor of default sink), interleave to stereo PCM s16 at 48 kHz, ALWAYS write a tmpfs WAV at `$XDG_RUNTIME_DIR/furugura/<id>/meeting.wav` for downstream consumers (whisper-stream tail, batch finalization, optional retention).

**Requirements:** R1, R3.

**Dependencies:** U1.

**Files:**
- Create: `src/audio/mod.rs`, `src/audio/capture.rs`, `src/audio/interleave.rs`, `src/audio/source_resolve.rs`, `src/audio/wav_writer.rs`
- Test: `tests/audio_capture_test.rs`

**Approach:**
- Source resolution: parse `pactl list short sources` for `.monitor` suffix; pick the monitor of the default sink (`default.audio.sink` PipeWire metadata or `pactl info | grep 'Default Sink'`). Fail-fast with a clear error if the user has no default sink.
- `tokio::process::Command::new("pw-record")` × 2: `--rate 48000 --channels 1 --format s16 --raw -` (stdout). `kill_on_drop(true)`.
- Read fixed-size frames (~2 ms at 48 kHz) from each subprocess; interleave to stereo; tee into (a) a `tokio::sync::broadcast` channel for `whisper-stream` consumer (U3), and (b) a WAV writer that always streams to `$XDG_RUNTIME_DIR/furugura/<id>/meeting.wav` with a standard RIFF/WAV header.
- Sample-clock alignment: lock both subprocesses to 48 kHz (PipeWire's native rate); compare frame counters every 5s; if drift > 50 ms, log warning and set `capture_quality: degraded` for the lifecycle state to record in frontmatter at finalize.
- Channel-split prior detection: at start, query `pactl list short sinks` for the default sink type; if it's `analog-output-speaker` (laptop speakers, no headphones), warn ("speaker bleed risk: mic may capture remote audio"). Bluetooth profile switch / USB hot-plug mid-meeting: detect via `pw-record` EOF; attempt re-resolution on the same source name before failing the capture.
- `--keep-audio` is NOT consulted in U2; U2 always writes the WAV. U10 decides preserve-vs-unlink at finalize.

**Patterns to follow:**
- `tokio::process::Command` with `kill_on_drop(true)`.
- `tokio::sync::broadcast` for tee-to-multiple-consumers; small `mpsc` for raw control.

**Test scenarios:**
- *Happy path:* Given both subprocesses spawned, when 1s of frames is consumed, then the WAV grows by 192,000 bytes and the broadcast emits matching frames.
- *Happy path:* Given `pactl info` reports a default sink with a `.monitor` source, when source resolution runs, then it returns the monitor source name; no error.
- *Edge case (mid-meeting device switch):* Given a `pw-record` subprocess EOFs (BT profile switch), when the reader detects EOF, then capture attempts re-resolution by source name and resumes; if re-resolution fails, captures stops gracefully and surfaces a recoverable error.
- *Edge case (no headphones):* Given the default sink is an analog laptop speaker (no headphones connected), when capture starts, then a warning is emitted ("speaker bleed risk").
- *Error path:* Given `pw-record` is not on `$PATH`, capture returns an error pointing the user to `furu setup`.
- *Integration:* Given a 30-second simulated meeting, `meeting.wav` is a valid stereo WAV with mic on left, system on right.

**Verification:** `cargo test --test audio_capture_test` passes; manual smoke produces a stereo WAV.

---

- U3. **Transcription pipeline (whisper.cpp streaming + batch + pyannote diarization)**

**Goal:** Run a streaming whisper.cpp subprocess during the meeting (consuming the broadcast channel from U2) writing low-latency segments to `transcript.live.jsonl`. At finalize time, run a batch whisper.cpp pass on the full WAV for accuracy, then pyannote.audio standalone for diarization, then merge into the authoritative `transcript.jsonl` and persist `<id>.diarization.rttm`.

**Requirements:** R4, R6, R7.

**Dependencies:** U1, U2.

**Files:**
- Create: `src/transcribe/mod.rs`, `src/transcribe/whisper_stream.rs`, `src/transcribe/whisper_batch.rs`, `src/transcribe/pyannote.rs`, `src/transcribe/jsonl.rs`, `src/transcribe/rttm.rs`
- Test: `tests/transcribe_test.rs`

**Approach:**
- **Streaming pass (during meeting):** spawn `whisper-stream --model <stream-model.gguf> --step 500 --length 5000 --keep 200 --vad-thold 0.6` reading PCM from stdin (fed by U2's broadcast tee). Default stream model: `base.en` GGUF (fast, low-accuracy; overwritten by batch pass). Vulkan-accelerated by default (`-ngl auto` or whisper.cpp's Vulkan build). Output JSONL lines: `{"t":"HH:MM:SS.mmm", "speaker":null, "text":"...", "start":..., "end":...}` to `transcript.live.jsonl`. Speaker is always `null` in the streaming pass; diarization happens at finalize.
- **Batch pass (at finalize):** spawn `whisper-cli <wav> --model <batch-model.gguf> --output-json --output-file <tmp> -t <threads>` against `meeting.wav`. Vulkan-accelerated. Default batch model: `large-v3` GGUF. Parses JSON output to per-segment lines.
- **Diarization:** spawn `python -m pyannote_runner <wav>` (a thin Python wrapper installed at setup) which runs `Pipeline.from_pretrained("pyannote/speaker-diarization-3.1", use_auth_token=$HF_TOKEN)` and emits RTTM. **HF token is passed via subprocess environment**, not CLI flag (avoids `/proc/<pid>/cmdline` exposure). Runs on CPU when CUDA absent — acceptable for a once-per-meeting batch.
- **Merge:** align whisper.cpp segments (timestamps + words) with pyannote RTTM segments by timestamp overlap; emit `transcript.jsonl` with per-segment `speaker: SPEAKER_NN` labels. Word-level timestamps may be missing for digits/symbols (whisper.cpp issue) — tolerate with `null` start/end at the word level; segment-level is always present.
- **Authoritative output:** `transcript.jsonl` (segments with speaker labels), `<id>.diarization.rttm` (segments-only sidecar in standard RTTM format).
- HF token absent: streaming and batch passes still run; diarization step is skipped with warning emitted to `transcript.jsonl` as `{"meta":"diarization_skipped","reason":"hf_token_missing"}`.

**Patterns to follow:**
- Tolerant `serde_json` deserialization for whisper.cpp output.
- `tokio::process::Command::env("HF_TOKEN", token_value)` for credential injection.

**Test scenarios:**
- *Happy path (streaming):* Given U2 emits a 60-second test audio, when `whisper-stream` runs, then `transcript.live.jsonl` grows in real-time with low-latency segments.
- *Happy path (batch+diarize):* Given a finalized WAV with two speakers, when batch + diarize runs, then `transcript.jsonl` segments have `SPEAKER_00`/`SPEAKER_01` labels and `<id>.diarization.rttm` is RTTM-valid.
- *Acceptance gate (diarization quality, AE3 + success criterion):* Given a 4-speaker fixture call (one local mic + three remote), when finalize runs, then ≥80% of transcript segments are correctly attributed by speaker (measured against a hand-labeled ground truth).
- *Edge case:* Given words containing digits (`"$2014"`), when parsed, then those words have `null` `start`/`end` and segment timestamps are still present.
- *Error path:* Given HF token absent, when finalize runs with `--diarize`, then diarization is skipped, transcript.jsonl carries the meta line, and the rest of the pipeline continues.
- *Error path:* Given `whisper-cli` is not on `$PATH`, when finalize starts, then an error points the user to `furu setup`.
- *Integration:* Given a 30-min channel-split fixture (left=mic, right=system), the diarization labels overwhelmingly map mic→one speaker, system→others.

**Verification:** Test fixtures round-trip cleanly; RTTM validates against pyannote.metrics.cli; ≥80% diarization accuracy on the 4-speaker fixture is the v1 ship gate.

---

### Phase 3 — Setup & Lifecycle

- U4. **First-run setup (`furu setup`)**

**Goal:** Verify external deps (`whisper-cli`, `whisper-stream`, `pyannote_runner` Python wrapper, `ollama`, `pw-record`, `ffmpeg`, `pactl`); walk HF token acquisition; persist token at `~/.config/furugura/hf_token` mode 0600; detect Vulkan and warn if absent.

**Requirements:** Enables R4, R5.

**Dependencies:** U1.

**Files:**
- Create: `src/cli/setup.rs`, `scripts/install_pyannote_runner.sh`
- Test: `tests/setup_test.rs`

**Approach:**
- Run external commands and check exit codes: `whisper-cli --help`, `whisper-stream --help`, `python -c 'import pyannote.audio'`, `ollama list`, `pw-record --help`, `ffmpeg -version`, `pactl --version`. For each, on failure, print the install hint (`paru -S whisper.cpp-vulkan`, `pipx install pyannote.audio`, `pacman -S ollama ffmpeg pipewire`). The pipx-installed pyannote is wrapped by a thin script (`pyannote_runner`) the project ships under `scripts/`.
- HF token flow: print step-by-step prompt walking the user to https://huggingface.co/pyannote/speaker-diarization-3.1 and `https://hf.co/settings/tokens`; read the token from stdin; write to `~/.config/furugura/hf_token`; `chmod 0600`.
- Vulkan detection: invoke `vulkaninfo --summary | head -1` (success ⇒ Vulkan available). Warn loudly if absent (`whisper.cpp will run on CPU; transcription will be ~10× slower`). Suggest installing `vulkan-intel` / `vulkan-radeon` / `nvidia-utils` per detected vendor.
- Idempotent — re-running skips already-completed steps.

**Patterns to follow:** `tokio::process::Command::new(…).status().await`; `std::os::unix::fs::PermissionsExt::set_mode(0o600)`.

**Test scenarios:**
- *Happy path:* Given all deps installed and a valid token, when setup runs, then the token file exists 0600 and `furu setup` re-runs print "already configured".
- *Edge case:* Given `whisper-cli` missing but `ollama` present, when setup runs, then it prints the whisper.cpp install hint and continues to the token step.
- *Error path:* Given the user enters an empty token 3 times, when setup validates, then it exits with a clear message.

**Verification:** `furu setup` followed by `furu start --dry-run` reports "all systems ready".

---

- U5. **Capture lifecycle (`furu start` / `furu stop` / state machine)**

**Goal:** Acquire `flock` on `active-meeting.json`, bind UDS at `furugura.sock`, orchestrate U2 + U3-streaming during the meeting, and run the full finalize pipeline (U3-batch + U3-pyannote + U9 + U10) in background after `furu stop`. Lifecycle states: `capturing` → `finalizing` → `done`.

**Requirements:** R2.

**Dependencies:** U2, U3, U9 (summary), U10 (markdown writer).

**Files:**
- Create: `src/cli/start.rs`, `src/cli/stop.rs`, `src/cli/cleanup.rs`, `src/cli/finalize.rs`, `src/lifecycle/mod.rs`, `src/lifecycle/lockfile.rs`, `src/lifecycle/uds.rs`
- Test: `tests/lifecycle_test.rs`

**Approach:**
- `furu start` flow:
  1. `mkdir -p $XDG_RUNTIME_DIR/furugura/<id>/` with mode 0700; assert resulting dir mode 0700 + uid match.
  2. Open `active-meeting.json`; `flock(LOCK_EX|LOCK_NB)`. If lock fails, print existing meeting id and exit.
  3. Write `{id, started_at, state: "capturing", notes_path, audio_kept, attendees}` to the file.
  4. Create `notes.live` (touch); record absolute path in lockfile's `notes_path`.
  5. Bind UDS at `furugura.sock`; `chmod 0600` immediately after bind. Protocol: line-delimited JSON. Methods: `mark` (orchestrator-side append to notes.live), `stop` (transition to `finalizing`), `status` (current state + elapsed).
  6. Spawn capture pipeline (U2) and streaming transcription (U3-streaming).
  7. Block on UDS until `stop` received (or SIGINT).
- `furu stop` flow:
  1. Read `active-meeting.json`; connect to UDS; send `{"method":"stop"}`. Server responds immediately `{"ok":true,"state":"finalizing"}`.
  2. `furu stop` exits immediately. The orchestrator transitions state to `finalizing` and runs the finalize pipeline in background.
- Server-side `stop` handler:
  1. Halt capture (U2) cleanly; close `meeting.wav`.
  2. Halt streaming whisper.cpp (U3-streaming).
  3. Update lockfile `state: "finalizing"`.
  4. Run U3-batch + U3-pyannote on `meeting.wav`.
  5. Run U9 (summarization) on the merged transcript + `notes.live` content.
  6. Run U10 (markdown finalization) — assemble + atomic-write `meeting.md` and copy `<id>.diarization.rttm`.
  7. Audio retention: if `--keep-audio`, run `ffmpeg -i meeting.wav -c:a libopus audio.opus` and move to durable dir. Otherwise `unlink meeting.wav`.
  8. Update lockfile `state: "done"`; release flock; remove socket; remove runtime dir.
- A second `furu stop` on a `finalizing` meeting prints progress (state + elapsed since `finalizing`).
- A `furu start` on a `finalizing` lockfile prints "previous meeting still finalizing — wait or run `furu finalize <id> --abort`" and exits.
- Crash recovery: stale lockfile detected (non-blocking `flock` succeeds, lockfile content present) → `furu start` prints `Stale meeting state found for <id>; run 'furu cleanup' to discard or 'furu finalize <id>' to retry the post-stop pipeline`. No interactive recovery prompt in v1.
- `furu cleanup [<id>]`: unlinks lockfile, socket, runtime dir.
- `furu finalize <id>`: re-runs the finalize pipeline against a retained `meeting.wav` (only meaningful with `--keep-audio` from the original start).

**Patterns to follow:**
- `nix::fcntl::flock` for the lock; `tokio::net::UnixListener` for the socket.
- Tokio cancellation tokens to coordinate shutdown across capture, streaming, and finalize branches.

**Test scenarios:**
- *Happy path:* Given `furu start` succeeded, when `furu stop` is invoked, then `furu stop` returns within 100ms with `{state: finalizing}`; eventually `~/Meetings/<id>/meeting.md` exists.
- *Happy path:* Given `--keep-audio`, when finalize runs, then `~/Meetings/<id>/audio.opus` exists alongside `meeting.md`.
- *Edge case:* Given a `furu start` is already running, when a second `furu start` runs, then it exits with "meeting already in progress: <id>".
- *Edge case:* Given `furu stop` ran, when a second `furu stop` runs before finalize completes, then it prints state + elapsed.
- *Edge case (crash recovery):* Given SIGKILL of the start process, when next `furu start` runs, then it prints the stale-state message and the user has explicit `furu cleanup` / `furu finalize` paths.
- *Error path:* Given UDS bind fails (permissions/conflict), when `furu start` runs, then it prints the conflict and exits without partial state.
- *Integration (AE1, AE2):* Full end-to-end with and without `--keep-audio`.

**Verification:** simulated 60-second meeting round-trips through start → stop → markdown without intervention; `furu stop` returns immediately; finalize completes in background.

---

- U6. **Marker IPC (`furu mark`)**

**Goal:** From a separate terminal, send a timestamped marker to the orchestrator over UDS (primary path) with a direct file-append fallback. The orchestrator handles the actual append to `notes.live`, sequencing it correctly relative to `furu stop`.

**Requirements:** R8.

**Dependencies:** U5.

**Files:**
- Create: `src/cli/mark.rs`
- Test: `tests/mark_test.rs`

**Approach:**
- Read `$XDG_RUNTIME_DIR/furugura/active-meeting.json` (no flock — read-only). Extract `notes_path` and `started_at`.
- **Path safety:** canonicalize `notes_path` (`std::fs::canonicalize`); assert it is a strict prefix-child of `$XDG_RUNTIME_DIR/furugura/`. Reject and exit if it escapes (defends against TOCTOU lockfile manipulation).
- Compute relative timestamp `HH:MM:SS` from `now() - started_at`.
- **Primary path (UDS):** connect to `furugura.sock`; send `{"method":"mark","text":"...","t":"HH:MM:SS"}`. Orchestrator appends to `notes.live`. Returns ack on success. Orchestrator refuses new marks once `state: finalizing`.
- **Fallback path:** if UDS unreachable (socket missing, connect refused), append `[HH:MM:SS] <text>\n` directly to `notes_path` with `O_APPEND | O_WRONLY | O_CREAT`. Print warning ("UDS unavailable; wrote directly").

**Patterns to follow:**
- `std::fs::canonicalize` + path-prefix assertion.
- `tokio::net::UnixStream` with short connect timeout (e.g., 200 ms).

**Test scenarios:**
- *Happy path:* Given a meeting in progress, `furu mark "follow up Sarah Tue"` at 14:23:11 (started 14:00:00) → orchestrator appends `[00:23:11] follow up Sarah Tue` to `notes.live`. Covers AE5.
- *Happy path (UDS down, fallback):* Given the UDS socket has been removed mid-meeting (rare), when `furu mark` runs, then it falls back to direct append with a warning.
- *Edge case (path injection):* Given a malicious process modified `active-meeting.json` so `notes_path` points to `~/.bashrc`, when `furu mark` runs, then canonicalization fails the prefix check and `furu mark` exits with an error.
- *Edge case (race vs finalize):* Given `furu stop` triggered finalization, when `furu mark` runs after, then UDS returns `{"ok":false,"reason":"finalizing"}` and `furu mark` prints "meeting is finalizing; mark dropped".
- *Edge case:* Given two `furu mark` invocations from two terminals via UDS, both lines appear in `notes.live` in arrival order (orchestrator serializes).
- *Error path:* Given no meeting in progress (lockfile absent), when `furu mark` runs, then it prints "no active meeting" and exits 1.

**Verification:** `furu start`, fire `furu mark "test"` from another terminal, verify the line is in `notes.live`; `furu stop` and confirm the line is in the final markdown's `## Notes` section.

---

### Phase 4 — Live UX

- U7. **Live transcript TUI (`furu live`)**

**Goal:** Tail `transcript.live.jsonl` for the active meeting; render in a Ratatui TUI with scrollback and `/`-substring search.

**Requirements:** R7.

**Dependencies:** U3 (streaming output).

**Files:**
- Create: `src/cli/live.rs`, `src/live/tui.rs`
- Test: `tests/live_test.rs`

**Approach:**
- Read `active-meeting.json`; locate `transcript.live.jsonl`. If absent (no active meeting), exit with clear message.
- Spawn a tokio task that watches the JSONL file (`notify` crate or polling at 250 ms) and pushes parsed segments into a tokio `mpsc` channel.
- Ratatui app: viewport widget showing the most recent N segments; status bar pinned at the bottom with elapsed time, current state (`capturing` / `finalizing`), and a `[REC]` indicator. Use `ratatui::insert_before` for stream-friendly append.
- Substring search: state lives on the `App` struct (no separate module). `/` enters search mode; filter visible segments to those containing the query; `n`/`N` jump next/prev; ESC clears.
- Speaker labels are `null` during streaming (whisper.cpp stream pass without diarization) — render as `(speaker unknown)` with neutral color. After finalize, this view is no longer the source of truth — `furu edit <id>` opens the merged markdown.
- Marker rendering: `furu mark` events are NOT rendered live in v1 (deferred to v1.5).
- Key bindings: `q` quit, `↑/↓` and `PgUp/PgDn` scroll, `/` enter search mode.

**Patterns to follow:** Ratatui v0.30 `App` pattern with `tokio::select!`; `crossterm` for terminal init/restore.

**Test scenarios:**
- *Happy path:* Given `transcript.live.jsonl` with 100 lines, when `furu live` opens, the last screenful is visible; `↑` scrolls. Covers AE4.
- *Happy path:* `/foo` filters to matching segments; `n` advances; ESC clears.
- *Edge case:* Given the JSONL is rotated (shouldn't happen in v1), when the TUI detects size shrunk, then it reseeks and re-renders without crashing.
- *Edge case:* Given the meeting transitions to `finalizing` while `furu live` is open, the status bar updates to `[FINALIZING]` and stops accepting new lines.
- *Error path:* Given no active meeting, `furu live` exits with a clear message and does not enter the TUI.

**Verification:** Manual: start a meeting, run `furu live` in another tmux pane, verify segments appear within ~1s of being spoken.

---

### Phase 5 — Review

- U8. **Meeting list & edit (`furu list`, `furu edit`)**

**Goal:** Discover past meetings under `~/Meetings/`, render a list with frontmatter metadata, open a selected meeting in `$EDITOR`.

**Requirements:** R12.

**Dependencies:** U10.

**Files:**
- Create: `src/cli/list.rs`, `src/cli/edit.rs`
- Test: `tests/list_edit_test.rs`

**Approach:**
- `furu list [--since <duration>]`: glob `~/Meetings/*/meeting.md`. Filter by filename date prefix (the ID stem encodes date) BEFORE opening files when `--since` is set, to avoid parsing unmodified history. Parse YAML frontmatter via `gray_matter`. Sort by `start_time` desc. Render id, date, duration, attendee count, summary headline.
- Soft pagination at 100 meetings shown by default; `--limit N` and `--all` flags.
- `furu edit <id>`: resolve `~/Meetings/<id>/meeting.md`; exec `$EDITOR` (fallback `vi`). After editor exits, no auto-reformat.
- `id` resolution: full id, partial-suffix, or shortlist offset (`furu edit 1` = most recent).

**Patterns to follow:** `gray_matter` for frontmatter parsing; `std::process::Command::new(editor).status()` with TTY inheritance.

**Test scenarios:**
- *Happy path:* 5 meetings → `furu list` prints sorted newest-first.
- *Happy path:* `furu edit 2026-05-02-1430-team-standup` opens correct file in `$EDITOR=vim`.
- *Edge case:* No meetings → `furu list` prints "no meetings found" without error.
- *Edge case:* Multiple matches for partial id → disambiguation list, exit.
- *Error path:* `$EDITOR` unset and no `vi` → clear message about setting `$EDITOR`.

**Verification:** `furu list` shows recent meetings; `furu edit <id>` opens correct file.

---

### Phase 6 — Summary & Output

- U9. **Summarization (Ollama HTTP client + cloud opt-in + consent gate)**

**Goal:** Send transcript + user notes to Ollama (default) or to a cloud LLM provider (opt-in with consent gate); parse the structured response into the `## Summary` block.

**Requirements:** R5, R11.

**Dependencies:** U3 (transcript), U6 (notes).

**Files:**
- Create: `src/summarize/mod.rs`, `src/summarize/ollama.rs`, `src/summarize/anthropic.rs`, `src/summarize/prompt.rs`
- Test: `tests/summarize_test.rs`

**Approach:**
- **Endpoint resolution (Ollama path):** check `OLLAMA_HOST` env, then config `summary_endpoint`, then default `http://localhost:11434`. If the resolved host is not loopback (not `127.0.0.1`, `::1`, or `localhost`), emit a warning before proceeding (`Warning: Ollama endpoint is not local (<host>); transcript content will leave this machine`). Honor user choice — don't refuse — but make it visible.
- **Local path:** HTTP POST to `<endpoint>/api/generate` with `{"model": "<from config>", "prompt": "<assembled>", "options": {"num_ctx": 16384, "temperature": 0.2}, "keep_alive": "30m", "stream": false}`.
- **Cloud opt-in:** when `--cloud-model claude-…` is passed OR `summary_provider: cloud:anthropic` is in config, route to Anthropic's API. Token from `~/.config/furugura/anthropic_token` mode 0600.
- **Consent gate (cloud only):** before invoking, print: `Notice: transcript will be sent to Anthropic API (claude-...). Press Enter to continue or Ctrl-C to abort`. `--yes` flag skips for scripted use. When `--attendees` non-empty AND no `--i-have-consent` flag, refuse with: `Refusing cloud summary: meeting has named attendees who have not consented to third-party processing. Pass --i-have-consent to override`.
- **Prompt template (`prompt.rs`):** system instructions ("You are a meeting note-taker. Output exactly three H3 sections: '### Decisions', '### Action items', '### Key points'. Each is a bullet list. Action items use GitHub task-list syntax: `- [ ] <name> — <task> (due <YYYY-MM-DD>)` when due date mentioned.") + verbatim user notes + diarized transcript.
- Parse the response: extract three H3 sections; emit a `SummaryBlock` for U10. Tolerate slight format drift (`Action Items` vs `Action items`).
- Frontmatter inputs from this unit: `summary_provider: local|cloud:<provider>`, `summary_model: <model>`, `data_egressed: none|full_transcript`.

**Patterns to follow:** `reqwest::Client` (single instance reused); tolerant heading-variant parsing.

**Test scenarios:**
- *Happy path (local):* Given a 30-min transcript and a localhost Ollama, summary parses into three non-empty H3 sections.
- *Happy path (cloud, scripted):* Given `--cloud-model claude-… --yes` and a valid token, summary returns from Anthropic and parses cleanly.
- *Edge case (no action items):* Section exists with `- _no action items_` placeholder so schema is consistent.
- *Edge case (loopback warning):* Given `OLLAMA_HOST=https://server.example.com:11434`, when summarize runs, warning surfaces; summary still proceeds.
- *Error path (cloud + attendees, no override):* Given `--cloud-model claude-…` AND `--attendees alice@…`, summarize refuses with the clear "named attendees" error.
- *Error path (Ollama down):* Clear error pointing user to `ollama serve` or `furu setup`.

**Verification:** Test fixtures pass three-H3 structural check; latency on 30-min fixture <30s on Vulkan-class GPU; consent gate fires only on cloud path.

---

- U10. **Markdown finalization**

**Goal:** Assemble the final `meeting.md` from frontmatter (with audit fields) + notes + summary + diarized transcript; persist RTTM sidecar and (when `--keep-audio`) the encoded audio file.

**Requirements:** R9, R10, R11.

**Dependencies:** U3 (transcript JSONL → prose), U6 (notes file via U5), U9 (summary).

**Files:**
- Create: `src/output/mod.rs`, `src/output/markdown.rs`
- Test: `tests/output_test.rs`

**Approach:**
- Frontmatter (snake_case, ISO 8601, `furugura_version: 1`):
  ```yaml
  ---
  furugura_version: 1
  id: 2026-05-02-1430-team-standup
  date: 2026-05-02
  start_time: 2026-05-02T14:30:00-04:00
  end_time: 2026-05-02T15:02:00-04:00
  duration_minutes: 32
  attendees: []
  audio_retained: false
  capture_quality: clean             # or "degraded"
  transcription_engine: whisper.cpp:large-v3
  diarization_model: pyannote-3.1
  summary_provider: local            # or "cloud:anthropic"
  summary_model: ollama:gemma3:4b
  data_egressed: none                # or "full_transcript"
  tags: [meeting]
  ---
  ```
- Body assembler:
  - `# <Title>` derived from id slug or `--title` flag.
  - `## Notes` — literal contents of `notes.live`. If empty, render `_(no notes captured)_`.
  - `## Summary` (H2) containing three H3 subsections (`### Decisions`, `### Action items`, `### Key points`) from U9's `SummaryBlock`. Missing H3s get `_no decisions_` / `_no action items_` / `_no key points_` placeholders so schema is preserved.
  - `## Transcript` — JSONL → `[HH:MM:SS] **SPEAKER_NN:** text` lines.
- Atomic write: temp file + `fsync` + `rename`.
- RTTM sidecar: copy `<id>.diarization.rttm` from runtime dir into `~/Meetings/<id>/`.
- Audio handling: if `--keep-audio`, run `ffmpeg -i meeting.wav -c:a libopus audio.opus`; move to durable dir. Otherwise `unlink meeting.wav`.
- Frontmatter writer is a private function inside `markdown.rs` (no separate module).

**Patterns to follow:** `tempfile` crate atomic-write pattern; avoid `std::fs::write` for the final file.

**Test scenarios:**
- *Happy path:* Given known JSONL/notes/summary, `finalize()` produces a golden-fixture-matching `meeting.md`. Covers AE5.
- *Happy path (audio default):* `audio_retained: false`, no audio files anywhere. Covers AE1.
- *Happy path (audio retained):* `audio_retained: true`, `~/Meetings/<id>/audio.opus` exists. Covers AE2.
- *Edge case:* Empty notes → `## Notes` body `_(no notes captured)_`.
- *Edge case:* Missing summary H3 → placeholder inserted; schema preserved.
- *Edge case (cloud audit):* Given U9 ran cloud, frontmatter reflects `summary_provider: cloud:anthropic` and `data_egressed: full_transcript`.
- *Edge case (capture degradation):* Given U2 set `capture_quality: degraded`, frontmatter reflects it; `furu stop` output mentions degradation.
- *Integration (F4 / R10 / agentic ingestion):* `gray-matter` parses frontmatter cleanly; downstream tools (Talos, Obsidian) read fields without LLM-parsing prose.

**Verification:** Fixture-based byte-identical `meeting.md`; cloud-path frontmatter audit fields populated correctly; `gray-matter` Ruby gem parses cleanly.

---

## System-Wide Impact

- **Interaction graph:** `furu start` is the orchestrator; `furu mark` and `furu live` are independent processes that read shared state under `$XDG_RUNTIME_DIR/furugura/<id>/`. UDS coordinates marker delivery and lifecycle transitions.
- **Error propagation:** every subprocess (`pw-record`, `whisper-stream`, `whisper-cli`, `pyannote_runner`, `ollama`, `ffmpeg`) is monitored. Clean failure surfaces a recoverable error to the lifecycle layer. SIGKILL → `furu cleanup` / `furu finalize` paths.
- **State lifecycle risks:** the lockfile must be released on every exit path. Use Rust's `Drop` impl on a `Lockfile` guard. Audio retention is latched at `furu start`; do not allow flipping mid-meeting. Lifecycle states are linear (`capturing` → `finalizing` → `done`); no rollback from `finalizing`.
- **API surface parity:** the markdown frontmatter schema is the "API". `furugura_version: 1` discriminates; v1.5 will document forward-compat behavior in its own plan.
- **Integration coverage:** the U5 + U10 round-trip is the load-bearing integration test. Cloud-vs-local audit-field test ensures privacy-affecting behavior is observable in the artifact. Diarization-accuracy fixture is the v1 ship gate.
- **Unchanged invariants:** none (greenfield project).

---

## Risks & Dependencies

| Risk | Mitigation |
|---|---|
| whisper.cpp Vulkan backend not yet on user's box. | `furu setup` checks `vulkaninfo`; warns and offers CPU fallback. Vulkan is part of standard Mesa on Arch — usually a one-package install (`vulkan-intel` / `vulkan-radeon` / `nvidia-utils`). |
| HF token gate breaks first-run UX for diarization. | `furu setup` walks the token acquisition; non-token fallback is non-diarized transcription with a `transcript.jsonl` meta line. |
| pyannote on CPU is slow. | Acceptable for a once-per-meeting batch. Document expected wait. CUDA-based pyannote is opportunistic acceleration when available. |
| Two parallel `pw-record` subprocesses drift in sample-clock alignment. | Lock to 48 kHz; runtime drift check; `capture_quality: degraded` when threshold exceeded; surface in `furu stop` output. Always-write WAV ensures the audio is captured even on drift; downstream consumers can inspect quality. |
| Channel-split prior breaks under speaker bleed (laptop speakers, no headphones) or mid-meeting device switch. | Detect at start (warn if no headphones/BT output); detect EOF on `pw-record` mid-meeting and attempt re-resolution. Document channel-split as a hint, not a guarantee. |
| Ollama is not running OR `OLLAMA_HOST` points remote. | `furu setup` checks. `summarize()` warns on non-loopback. Cloud opt-in has explicit consent gate. |
| `flock` releases on process death; on-disk lockfile content can be stale. | Stale-state detection: non-blocking `flock` succeeds → previous holder is dead → user runs `furu cleanup` or `furu finalize`. No interactive prompt. |
| Markdown schema drift across versions breaks downstream consumers. | `furugura_version: 1` discriminator; v1 implementations tolerate unknown fields (forward-compat default); v1.5 plan defines its own migration story. |
| User runs a 6-hour all-hands and exhausts `$XDG_RUNTIME_DIR` (small tmpfs cap). | Detect low free space at start; ~350 MB / 30 min is comfortable on default ~3 GB tmpfs (covers ~4 hours). Hard guarantees are v2 (chunked WAL). |
| Cloud LLM data egress on multi-attendee meetings without consent. | Refuse cloud path when `--attendees` non-empty unless `--i-have-consent`. Frontmatter records `summary_provider` + `data_egressed` for audit. |
| Audio in tmpfs may swap to disk before unlink. | Startup `/proc/swaps` advisory at v1; `mlock`'d audio buffers in v2. Privacy framing is honest about this boundary. |
| HF token leakage on subprocess cmdline. | Pass via `HF_TOKEN` env var, not CLI flag. `/proc/<pid>/environ` is mode 0600 for the user only. |

---

## Documentation / Operational Notes

- README.md ships with: install instructions (`paru -S furugura-bin` once AUR PKGBUILD lands), a single-meeting walkthrough, the markdown schema reference (the v1 "format spec"), and a troubleshooting table for common first-run failures (HF token, no Vulkan, Ollama not running, `pw-record` not on `$PATH`).
- Privacy-affecting flags and config keys are explicitly documented: which configurations cause network transmission of meeting content (cloud `summary_provider`, non-loopback `OLLAMA_HOST`).
- `furu --help` and per-subcommand `--help` are the canonical CLI reference.
- No telemetry, no analytics, no auto-update check. Updates via package manager.

---

## Deferred / Open Questions

Findings from the 2026-05-02 doc-review pass that were deliberately deferred to follow-up rather than applied to this plan.

### From 2026-05-02 review

- **Markdown schema universality vs Talos-shaped assumptions** — the schema chose `**Decision:**` prefix and the three-H3 split partly because Talos extracts cleanly from those shapes. A second downstream consumer (Obsidian Dataview, agentic ingestor with different conventions) may motivate splitting "universal" vs "Talos-specific" fields in the README schema spec. Resolve when a second consumer surfaces. (Surfaced by product-lens.)
- **v1.5 voiceprint shipping cadence** — success criterion is "user stops switching to Mac from week 2." If recurring meetings re-tax the user with renames every week, v1.5 may need to ship within ~4 weeks of v1, not as a separate phase. Empirical question — depends on real v1 usage. Surface as explicit risk in v1's success-evaluation. (Surfaced by product-lens.)
- **`furugura_version` migration story** — v1 default is forward-compat ("implementations tolerate unknown fields"); concrete v1.5 schema migration plan written when v1.5 fields are designed. No v1 consumer branches on `furugura_version`. (Surfaced by adversarial.)

### FYI observations (from doc-review, recorded for context)

- **Identity drift if Hyprnote/Granola ship Linux** — Furugura's defensible identity is CLI-native + local-first + markdown-of-record + Talos-pipeline-friendly; if the audience ever broadens beyond N=1, articulate this explicitly. (product-lens, anchor 50)
- **v2 scope inflation** — daemon + wl-meetcap + MCP server are speculative scope on the v2 roadmap; re-examine after v1 + v1.5 actually ship. (product-lens, anchor 50)
- **Marker-only notes UX may not survive contact with paragraph-style note-takers** — for users whose Granola habit is paragraph notes, `furu mark` per note is high friction. Validate on real usage; consider `furu note` (no argument) opening `$EDITOR` on `notes.live`. (product-lens, anchor 50)
- **`furu list` scaling past 100s of meetings** — directory glob + frontmatter parse is O(N); filename-date filter is the easy first optimization (already in U8). Sidecar index becomes worth building past ~1000 meetings. (adversarial, anchor 50)
- **UDS socket / runtime dir defense-in-depth** — `chmod 0700 / 0600` on creation is now in U5; original concern was implicit assumption rather than an open vulnerability given `$XDG_RUNTIME_DIR` is already 0700. (security-lens, anchor 50)

---

## Sources & References

- **Origin document:** `docs/brainstorms/2026-05-02-furugura-v1-requirements.md`
- whisper.cpp: https://github.com/ggerganov/whisper.cpp
- pyannote 3.1: https://huggingface.co/pyannote/speaker-diarization-3.1
- pyannote/embedding: https://huggingface.co/pyannote/embedding
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
- Gemma 3: https://ollama.com/library/gemma3
- Ratatui v0.30: https://ratatui.rs/highlights/v030/
- pw-record: https://docs.pipewire.org/page_man_pw-record_1.html
- Granary (third-party Granola exporter — markdown turn-format reference): https://github.com/wassimk/granary
- XDG Base Directory: https://wiki.archlinux.org/title/XDG_Base_Directory
- Granola privacy: https://docs.granola.ai/help-center/policies/privacy-policy
