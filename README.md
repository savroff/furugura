# Furugura

CLI-only Linux meeting capture, transcription, and AI summary — a Granola-shaped tool for users who live in the terminal.

**フルグラ** (furugura) is a Japanese portmanteau of fruit + granola, popularized by Calbee's Frugra cereal. The framing: granola, but richer — an open-source remix shaped for the Linux power-user audience.

## Status

v1 feature-complete. Install from source for now (AUR `-bin` packaging is a follow-up). Tracks the [v1 implementation plan](docs/plans/2026-05-02-001-feat-furugura-v1-plan.md).

## What it does

- Captures mic + system audio simultaneously on PipeWire-based Linux (Omarchy / Hyprland is the dev target).
- Transcribes via [whisper.cpp](https://github.com/ggerganov/whisper.cpp) with Vulkan acceleration; diarizes via [pyannote.audio](https://github.com/pyannote/pyannote-audio).
- Summarizes locally via [Ollama](https://github.com/ollama/ollama) by default; cloud LLM (Anthropic) is opt-in with an explicit consent gate.
- Writes one Markdown file per meeting with structured YAML frontmatter, AI summary, and full diarized transcript — readable by humans and ingestible by downstream tools.
- Live transcript view (`furu live`) and timestamped manual markers (`furu mark "..."`) during the meeting.
- Audio is held in volatile (tmpfs) storage during the meeting and discarded by default; opt-in retention via `--keep-audio`.

## Install

### From source

```sh
git clone https://github.com/savroff/furugura.git
cd furugura
cargo install --path . --root ~/.local --locked
bash scripts/install_pyannote_runner.sh
```

`furu` lands in `~/.local/bin/furu`. Make sure `~/.local/bin` is on `$PATH`.

### External dependencies

On Arch / Omarchy:

```sh
paru -S whisper.cpp-vulkan
sudo pacman -S ollama ffmpeg pipewire libpulse vulkan-tools
systemctl --user enable --now ollama
ollama pull gemma3:4b
pipx install pyannote.audio
```

Then walk the first-run setup:

```sh
furu setup
```

Verifies every dependency, walks HuggingFace token enrollment for pyannote, and detects Vulkan. Idempotent — re-run safely.

## Quick walkthrough

In one terminal:

```sh
furu start --title "team standup"
```

Capture notes from another terminal as the conversation goes:

```sh
furu mark "follow up Sarah Tue"
furu mark "agreed on Q3 timeline"
```

Watch the live transcript in a third terminal (optional):

```sh
furu live
```

End the meeting:

```sh
furu stop
```

`furu stop` returns immediately. The original `furu start` terminal runs the finalize pipeline (batch transcribe → diarize → summarize → write markdown) and exits when done. The result lands at `~/Meetings/<id>/meeting.md`.

## Output

Per-meeting directory under `~/Meetings/<id>/`:

```
~/Meetings/2026-05-02-1430-team-standup/
├── meeting.md
├── transcript.jsonl
├── 2026-05-02-1430-team-standup.diarization.rttm   # if pyannote ran
└── audio.opus                                       # only with --keep-audio
```

### Markdown schema (the v1 format spec)

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

# Team Standup

## Notes

[00:00:00] kickoff
[00:01:23] Sarah's blocker

## Summary

### Decisions
- **Decision:** Adopt PipeWire as v1 audio target.

### Action items
- [ ] Sarah — write the README (due 2026-05-09)

### Key points
- whisper.cpp + Vulkan covers Intel Arc / AMD / NVIDIA.

## Transcript

[00:00:01.000] **SPEAKER_00:** Hello.
[00:00:02.500] **SPEAKER_01:** Hi.
```

Headings are stable across versions; `furugura_version: 1` is the schema discriminator. Downstream tools (Talos, Obsidian Dataview, agent toolchains) can extract decisions, action items, and attendees mechanically without LLM-parsing prose.

## Commands

| Command | Purpose |
|---|---|
| `furu start [--keep-audio] [--title T] [--attendees A,B] [--cloud-model M]` | Begin a meeting. Foreground process — keep its terminal open. |
| `furu stop` | End the active meeting. Returns immediately; `furu start`'s terminal runs finalize. |
| `furu mark "<text>"` | Append a timestamped note to the active meeting. |
| `furu live` | Tail the live transcript in a Ratatui TUI. `/` search, `j/k` scroll, `q` quit. |
| `furu list [--since 7d] [--limit N] [--all]` | List past meetings, newest first. |
| `furu edit <id>` | Open a meeting in `$EDITOR`. Accepts full id, partial suffix, or shortlist offset. |
| `furu setup [--check]` | First-run dependency walk-through. |
| `furu cleanup [<id>]` | Discard stale runtime state after a crash. |
| `furu finalize <id>` | Re-run the post-stop pipeline against retained audio. |

## Configuration

Defaults live in `~/.config/furugura/config.toml`. Every field has a sensible default; missing files use defaults silently.

```toml
whisper_model_batch  = "large-v3"
whisper_model_stream = "base.en"
summary_model        = "gemma3:4b"
summary_provider     = "local"            # or "cloud:anthropic"
num_ctx              = 16384
audio_rate           = 48000
keep_audio           = false
# output_dir       = "/path/to/dir"       # default: ~/Meetings
# summary_endpoint = "http://localhost:11434"
```

`FURU_*` environment variables override individual fields at runtime
(`FURU_SUMMARY_MODEL`, `FURU_OUTPUT_DIR`, `FURU_KEEP_AUDIO`, ...).

## Privacy posture

- **Audio is not retained by default.** Held in `$XDG_RUNTIME_DIR` (tmpfs) during the meeting, unlinked at finalize. `--keep-audio` opts in to a re-encoded `audio.opus` next to `meeting.md`.
- **Local LLM by default.** Ollama on `localhost:11434`. If `OLLAMA_HOST` resolves to a non-loopback address, `furu` warns before sending and records `data_egressed: full_transcript` in the frontmatter.
- **Cloud opt-in.** `--cloud-model claude-...` triggers an interactive consent gate. With named attendees, `--i-have-consent` is also required.
- **Honest about boundaries.** Tmpfs audio may page to swap on systems with active swap; v2 will `mlock` the buffers. Tokens (HuggingFace, Anthropic) live under `~/.config/furugura/` mode 0600.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `furu setup` reports `whisper-cli` missing | whisper.cpp not installed | `paru -S whisper.cpp-vulkan` |
| `furu start` warns "live transcript view disabled" | `whisper-stream` not on `$PATH` | Same as above |
| Diarization skipped at finalize | No HuggingFace token | Re-run `furu setup` |
| Summary skipped: "could not reach Ollama" | Ollama service not running | `systemctl --user enable --now ollama` |
| Capture flagged `degraded` | No headphones (mic captures speakers), or sample-clock drift between subprocesses | Plug headphones; check tmpfs free space |
| `another meeting is in progress` on second `furu start` | Stale lockfile after a crash | `furu cleanup` |
| `furu mark` says "no active meeting" | Lockfile gone or different login session | Run `furu start` first |

## Roadmap

- **v1.5** — Persistent voiceprint store keyed on email; vocabulary / `initial_prompt` injection.
- **v1.6** — Calendar polling for auto-start (CalDAV / Google Calendar).
- **v2** — `furud` daemon, `mlock`'d audio buffers, two-engine verification, MCP server (only if a downstream consumer demands it).

## Documentation

- [v1 requirements](docs/brainstorms/2026-05-02-furugura-v1-requirements.md) — what we're building and why.
- [v1 implementation plan](docs/plans/2026-05-02-001-feat-furugura-v1-plan.md) — how we're building it.

## License

MIT. See `LICENSE`.
