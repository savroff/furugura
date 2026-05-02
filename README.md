# Furugura

Meeting notes for people who live in the terminal.

You're on Linux. You have meetings. Granola is the tool everyone raves about, but it's macOS-only — so you keep flipping back to your Mac to get a transcript and an AI summary, and the moment of capture happens off your real workstation.

Furugura puts that loop on Linux. Press `furu start`, have your meeting, press `furu stop`. A minute or two later you have a Markdown file with a clean summary, the full diarized transcript, and any quick notes you typed during the call. Everything runs locally by default. The terminal is the UI.

The name is borrowed from **フルグラ** — the Japanese portmanteau of fruit + granola, popularized by Calbee's Frugra cereal. Granola, but richer.

---

## What a meeting looks like

You're about to join a call. Open a terminal:

```sh
furu start --title "team standup"
```

`furu` quietly starts capturing both your mic and the system audio coming back from Zoom, Meet, or whatever else is making sound. Join the call.

When somebody says something you want to remember, jump to another terminal:

```sh
furu mark "follow up with Sarah Tuesday"
furu mark "Q3 timeline locked"
```

Each line gets a timestamp and lands in your final notes verbatim.

If you want to glance at what's been said while you grab a coffee, open a third terminal:

```sh
furu live
```

That gives you a TUI with the live transcript. Press `/` to search, `j`/`k` to scroll, `q` to drop out.

When the call ends:

```sh
furu stop
```

`furu stop` returns immediately. The terminal where you started keeps running for another 30–60 seconds while it does the heavy lifting — running the full-accuracy transcription pass, identifying speakers, asking your local LLM for a summary, and writing everything to disk. Then it exits.

You'll find the result at `~/Meetings/<date>-<title>/meeting.md`.

---

## Status

v1 is feature-complete. Install from source for now; an AUR `-bin` package is on the to-do list. The full design lives in [docs/plans/2026-05-02-001-feat-furugura-v1-plan.md](docs/plans/2026-05-02-001-feat-furugura-v1-plan.md) if you want to know how everything fits together.

---

## Install

If you're on Arch / Omarchy, this is the whole story:

```sh
# 1. Build and install Furugura
git clone https://github.com/savroff/furugura.git
cd furugura
cargo install --path . --root ~/.local --locked
bash scripts/install_pyannote_runner.sh

# 2. Install the things Furugura calls out to
yay -S whisper.cpp-vulkan
sudo pacman -S ollama ffmpeg pipewire libpulse vulkan-tools python-pipx
systemctl --user enable --now ollama
ollama pull gemma3:4b
pipx install pyannote.audio

# 3. Walk first-run setup (HuggingFace token for diarization, etc.)
furu setup
```

`furu setup --check` is the verification command — it prints a green/red list of every dependency. Re-run anytime; it's idempotent.

---

## What ends up on disk

Everything for one meeting lives in its own directory:

```
~/Meetings/2026-05-02-1430-team-standup/
├── meeting.md                                     ← the canonical artifact
├── transcript.jsonl                               ← machine-readable transcript
├── 2026-05-02-1430-team-standup.diarization.rttm  ← raw pyannote output
└── audio.opus                                     ← only if you passed --keep-audio
```

`meeting.md` is the file you'll actually read and edit. Its shape is deliberately stable so other tools can ingest it:

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
capture_quality: clean
transcription_engine: whisper.cpp:large-v3
diarization_model: pyannote-3.1
summary_provider: local
summary_model: ollama:gemma3:4b
data_egressed: none
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

Three things to know about this format:

1. **The headings are the API.** `## Notes`, `## Summary`, `## Transcript`, and the three H3s under `Summary` won't move. Tools that ingest meetings can grep for them.
2. **The frontmatter is the audit trail.** `summary_provider`, `data_egressed`, and `audio_retained` make it obvious from the file alone what left your machine.
3. **Speaker labels start generic.** `SPEAKER_00`, `SPEAKER_01`, etc. Rename them by hand in `meeting.md` after the call. v1.5 will start remembering voices across meetings.

---

## Commands

| | |
|---|---|
| `furu start [--keep-audio] [--title T] [--attendees A,B] [--cloud-model M]` | Begin a meeting. Keep this terminal open. |
| `furu stop` | End the active meeting. Returns instantly; `furu start`'s terminal finalizes. |
| `furu mark "<text>"` | Append a timestamped note to the active meeting. |
| `furu live` | Open a TUI tailing the live transcript. |
| `furu list [--since 7d] [--limit N] [--all]` | List past meetings, newest first. |
| `furu edit <id>` | Open a meeting in `$EDITOR`. Accepts `team-standup`, `1` (newest), or the full id. |
| `furu setup [--check]` | First-run walk-through and dep verification. |
| `furu cleanup [<id>]` | Discard stale runtime state after a crash. |
| `furu finalize <id>` | Re-run the post-stop pipeline against retained audio. |

---

## Privacy

This part matters, so it's short and explicit:

- **Audio is not kept by default.** It lives in `$XDG_RUNTIME_DIR` (tmpfs) during the meeting and is unlinked when Furugura finalizes. Pass `--keep-audio` if you want a re-encoded `audio.opus` next to `meeting.md`.
- **Summaries are local by default.** Ollama on `localhost:11434`. If `OLLAMA_HOST` is set to anything else, Furugura warns you before sending and records `data_egressed: full_transcript` in the frontmatter.
- **Cloud LLMs are opt-in and consent-gated.** `--cloud-model claude-...` triggers an interactive prompt. If `--attendees` is non-empty, you also have to pass `--i-have-consent`.
- **Honest about edges.** On systems with active swap, tmpfs pages can hit disk before they're unlinked. v2 will `mlock` the buffers. HuggingFace and Anthropic tokens live under `~/.config/furugura/`, mode 0600.

---

## Configuration

Most people never need this. The defaults are at `~/.config/furugura/config.toml` and look like:

```toml
whisper_model_batch  = "large-v3"
whisper_model_stream = "base.en"
summary_model        = "gemma3:4b"
summary_provider     = "local"
num_ctx              = 16384
audio_rate           = 48000
keep_audio           = false
# output_dir       = "/path/to/dir"
# summary_endpoint = "http://localhost:11434"
```

Every field is overridable per-run via `FURU_*` env vars (`FURU_SUMMARY_MODEL`, `FURU_OUTPUT_DIR`, `FURU_KEEP_AUDIO`, …) or, for the ones that map to flags, on the `furu start` command line.

---

## When something goes sideways

| What you saw | What's probably wrong | What to try |
|---|---|---|
| `furu setup` says `whisper-cli` is missing | whisper.cpp isn't installed | `yay -S whisper.cpp-vulkan` |
| `furu start` warns "live transcript view disabled" | Same — `whisper-stream` ships in the same package | Same |
| Diarization skipped at finalize | No HuggingFace token saved | Re-run `furu setup` |
| Summary skipped: "could not reach Ollama" | Ollama service isn't running | `systemctl --user enable --now ollama` |
| Capture flagged `degraded` | No headphones (mic picks up speakers), or audio clock drift | Plug headphones; check tmpfs free space |
| `another meeting is in progress` on second start | Lockfile from a previous crash | `furu cleanup` |
| `furu mark` says "no active meeting" | Wrong session, or `furu start` never ran | Start a meeting first |

---

## What's coming

- **v1.5** — Voice memory. Rename a speaker once, and Furugura recognizes them in future meetings.
- **v1.6** — Calendar auto-start. Furugura notices a meeting is starting and runs itself.
- **v2** — Optional daemon, `mlock`'d audio buffers, two-engine transcript verification.

---

## License

MIT. See [`LICENSE`](LICENSE).
