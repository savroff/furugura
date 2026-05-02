---
date: 2026-05-02
topic: furugura-v1-requirements
---

# Furugura v1 Requirements

## Summary

Furugura v1 is a CLI-only Linux meeting capture tool that records mic + system audio, transcribes and diarizes via WhisperX, summarizes locally after the meeting, and writes a per-meeting markdown file containing the user's manual notes, the AI summary, and the full transcript. During the meeting, a scrollable searchable live transcript view and manual marker capture are first-class. Voiceprint persistence and calendar auto-trigger are deferred to v1.5+.

---

## Problem Frame

The user is a Linux power-user (Omarchy / Hyprland) who relies on Granola on macOS to capture meetings, generate AI summaries, and feed those summaries into a personal-knowledge pipeline (Talos MCP). On Linux, no equivalent exists — Granola has no Linux client, and no commercial competitor (Otter, Fireflies, tl;dv, Fathom, Read.ai) ships a Linux desktop app either. Existing OSS attempts each cover only one slice (Buzz: mic-only, Vibe: file-based, Screenpipe: 24/7 ambient, Meetily: Tauri-shell, Hyprnote: macOS-only) and none assemble the full Granola loop on Linux with output shaped for downstream agentic ingestion. Diarization quality is the universal weak spot in the field — Hyprnote's well-known "30-person meeting → one speaker" failure mode reflects pyannote's open-world clustering limits.

The cost shape: every meeting forces a context switch back to macOS, breaking the user's Linux-native workflow and adding friction to a tool used many times per week. The macOS Granola → Talos pipeline works but lives off-platform, which means the user's Linux-side daily setup (Omarchy, terminal, editor) is excluded from the moment when the most valuable knowledge is being captured.

---

## Actors

- A1. **Linux user (primary)** — runs `furu` commands during and around meetings, takes manual notes via marker gestures, reviews and corrects the markdown output afterward.
- A2. **Downstream consumer** — Talos MCP, Obsidian vault, manual reader, or future agentic tool that ingests Furugura's per-meeting markdown file. Furugura emits to disk; the consumer reads from disk. Furugura does not push.

---

## Key Flows

- F1. **Capture a meeting end-to-end (manual)**
  - **Trigger:** User is about to start a call.
  - **Actors:** A1
  - **Steps:** User runs `furu start` before joining the call. Furugura begins capturing mic + system audio and streaming transcription. During the meeting, user can run `furu live` in another terminal for a scrollable searchable view, and `furu mark "<text>"` to capture timestamped notes. Call ends; user runs `furu stop`. Furugura finalizes capture, generates the post-meeting summary from transcript + user notes, writes the markdown file to the configured output directory, and discards raw audio.
  - **Outcome:** One markdown file exists with frontmatter, user notes, AI summary, full transcript. Audio is gone (default). File is ready for any downstream consumer to ingest.
  - **Covered by:** R1, R2, R3, R4, R5, R7, R8, R9, R10, R11

- F2. **Catch up after a distraction (live view + search)**
  - **Trigger:** User missed the last few minutes of an active meeting (Slack ping, dog barked, side conversation).
  - **Actors:** A1
  - **Steps:** User opens a new terminal, runs `furu live`. The TUI shows the live-growing transcript with the most recent turns at the bottom. User scrolls up to find what they missed; uses substring search (`/`) to jump to a speaker name or topic.
  - **Outcome:** User has read the section they missed and rejoins oriented.
  - **Covered by:** R7

- F3. **Manual review and correction (post-meeting)**
  - **Trigger:** A meeting markdown file exists with generic speaker labels (`SPEAKER_00`, `SPEAKER_01`).
  - **Actors:** A1
  - **Steps:** User runs `furu list` to see recent meetings, then `furu edit <id>` which opens the file in `$EDITOR`. User renames speaker labels to real names in-place; cleans up summary if needed; saves. (In v1.5, this rename action also enrolls the renamed speakers into the persistent voiceprint store; v1 only does the in-file rename.)
  - **Outcome:** Markdown file has correct speaker names.
  - **Covered by:** R6, R12

- F4. **Hand-off to downstream consumer**
  - **Trigger:** A meeting markdown file is finalized in the configured output directory.
  - **Actors:** A2
  - **Steps:** Downstream tool (Talos, Obsidian sync, manual reader, future agent) reads the file. Frontmatter is parsable as YAML; section structure (`## Notes`, `## Summary`, `## Transcript`) is stable enough that the consumer can extract structured fields (decisions, action items, attendees) without parsing prose.
  - **Outcome:** Downstream tool ingests the meeting; user did not have to copy/paste, format-convert, or run an export step.
  - **Covered by:** R9, R10, R11

---

## Requirements

**Capture and audio**
- R1. Furugura captures mic and system audio simultaneously on PipeWire-based Linux. Omarchy / Hyprland is the dev target; other PipeWire-based distros are best-effort.
- R2. Capture begins via explicit CLI command (`furu start`) and ends via `furu stop`. No auto-trigger in v1.
- R3. By default, raw audio is held in volatile storage during the meeting and discarded at finalization. The user can opt in to retention via a flag.

**Transcription, diarization, and summary**
- R4. Transcription and speaker diarization use WhisperX, which produces a transcript with word-level timestamps and per-speaker labels.
- R5. Post-meeting summary is generated locally by default from transcript + user notes; cloud LLM APIs are an opt-in alternative per meeting.
- R6. Speaker labels in v1 output are generic identifiers (`SPEAKER_00`, `SPEAKER_01`, …) that the user can rename in the output file. No persistent voiceprint store in v1.

**During-meeting interaction**
- R7. A live transcript view (`furu live`) is invocable while the meeting is ongoing. The view is a TUI scrollable line-by-line, with basic substring search.
- R8. The user can capture timestamped notes/markers during the meeting via a CLI gesture (e.g., `furu mark "<text>"`). Each marker is preserved verbatim in the final markdown with its capture timestamp.

**Output**
- R9. The output for each meeting is a single markdown file with YAML frontmatter, written to a configurable directory.
- R10. The frontmatter records meeting metadata: date, duration, attendees (best-effort, may be empty in v1), audio retention status, transcription model, summary model.
- R11. The body has three distinct sections, in this order: user manual notes (verbatim, timestamped), AI summary (decisions, action items, key points), full transcript (timestamps + speaker labels).

**Lifecycle and review**
- R12. The user can list past meetings (`furu list`) and open one for editing in `$EDITOR` (`furu edit <id>`).
- R13. Furugura ships as a single binary in v1. No daemon. Daemon work is deferred until calendar auto-trigger and MCP-feed concerns justify it.

---

## Acceptance Examples

- AE1. **Covers R3.** Given Furugura is started without an audio-retention flag, when `furu stop` runs, then no `.wav` / `.opus` / other audio file remains in the output directory or any other persistent location on disk.
- AE2. **Covers R3.** Given Furugura is started with an explicit retention flag (e.g., `--keep-audio`), when `furu stop` runs, then the audio file is preserved alongside the markdown file in the output directory.
- AE3. **Covers R4, R6.** Given a 4-person call, when `furu stop` runs, then the markdown's transcript section labels each speaker as one of `SPEAKER_00`–`SPEAKER_03`, and the user can rename these to real names by editing the file directly.
- AE4. **Covers R7.** Given a meeting is being captured (`furu start` ran, `furu stop` has not), when the user runs `furu live` in another terminal, then the TUI displays the live-growing transcript and supports scrollback and substring search against the lines already produced.
- AE5. **Covers R8, R11.** Given a meeting is being captured, when the user runs `furu mark "follow up Sarah Tue"` at 14:23:11, then a line `[14:23:11] follow up Sarah Tue` appears in the meeting's notes, and after `furu stop`, that exact line is in the `## Notes` section of the final markdown.
- AE6. **Covers R5.** Given the user has not specified a cloud LLM flag, when `furu stop` runs, then summary generation calls a local model (Ollama) and does not make network requests to any external LLM API.

---

## Success Criteria

- The user stops switching to macOS for meetings; Furugura is the default tool on Linux from week 2 of use onward.
- A meeting markdown file produced by Furugura is ingested by Talos (or another downstream tool) without manual format conversion or human cleanup before ingestion.
- Live transcript view and manual mark capture are usable enough during a real call that the user prefers them to taking notes elsewhere (separate vim file) or relying on memory.
- Speaker labels are correct enough on 2–6 person calls that the post-meeting rename feels like a finish, not a rebuild.
- Downstream agents (`ce-plan`, future implementers, Talos's auto-extract) can read the markdown's frontmatter and section structure mechanically — no need to LLM-parse prose to find decisions / action items / attendees.

---

## Scope Boundaries

### Deferred for later

- Persistent voiceprint store keyed on email — v1.5
- Calendar polling for auto-start (CalDAV / Google Calendar) — v1.6
- `furud` daemon (systemd `--user` service) — v2 once auto-trigger and MCP-feed needs justify it
- Standalone `wl-meetcap` upstream contribution as a stable JSON-RPC primitive — v2
- MCP server inside Furugura — v2 (and only if a Talos-coupled use case argues for it; today Talos is already the meeting MCP)
- Live AI summary during the meeting — flagged uncertain in synthesis; deferred until v2 evaluates need (live transcript + manual marks cover the catch-up case in v1)
- Numbers / names / acronyms vocabulary file with Whisper `initial_prompt` injection — v1.5
- Crash-safe append-only WAL chunked Opus capture — v2
- Stenographer-style two-engine verification (Whisper + Parakeet diff) — v2 quality option

### Outside this product's identity

- Vector search / personal-corpus query (Talos + QMD owns this; Furugura is a *feeder*, not a query layer)
- Spaced-repetition pings on commitments (downstream tool's job; Furugura emits structured commitments, not nudges)
- Hash-chained ELN-style append-only integrity log (over-engineered for this user; regulated industries use audited tools)
- Hardware foot-pedal / macropad UX (niche; doesn't fit Linux power-user norm)
- Host-side capture appliance (Pi / NUC SFU) — different audience, different topology
- Multi-user / shared-transcript / collaborative editing — single-user product
- macOS / Windows ports — Linux-only
- Browser extension as primary UX surface — CLI-only constraint; PipeWire `combine-sink` handles browser-tab audio at the audio layer
- GUI shell (Tauri / Electron / Qt) — CLI-only constraint
- OMF as a standalone JSON-LD spec with reference parsers ahead of any clients — markdown frontmatter is the v1 deliverable; standardize later if a second client demands it
- Editor-embedded live notes-spine (typed notes merge with transcript in real time) — manual marks are the CLI-friendly equivalent and are merged *after* the meeting, not during

---

## Key Decisions

- **Phased delivery (v1 → v1.5 → v1.6 → v2) over big-bang.** Feedback loop informs each layer; lowest stall risk; trades feature completeness for shipping speed. Avoids the well-known failure mode where an OSS meeting tool stalls on diarization tuning before shipping a single useful meeting note.
- **WhisperX as v1 transcription/diarization mechanism.** Collapses Whisper + pyannote + word-level alignment into a single MIT-licensed dependency; avoids reinventing diarization in v1. The dependency choice is committed, not a planning question.
- **Markdown + YAML frontmatter as output format.** Matches how Talos already ingests data; readable by humans; consumable by any tool with a markdown parser; no proprietary schema. The format is documented in the project README; not a separately-published spec.
- **Manual marks (`furu mark`) instead of editor-embedded live merge.** Keeps CLI-only ergonomics; manual notes are still preserved in the final artifact; merge happens *after* the meeting, not during. Avoids importing the tl;dv-style live-bullet UX that distracts during the call.
- **Single binary in v1; daemon deferred.** Removes systemd surface area until auto-trigger needs justify it. Lower install friction for early users.
- **"No audio persists by default" is a privacy preference, not a legal claim.** Two-party-consent law treats live transcription as a recording, so the architectural choice is preference for a clean default and a tighter blast radius — not consent-law theater.
- **Generic speaker labels in v1 with manual rename.** Cheaper than persistent voiceprints; the rename data structure becomes the seed for the v1.5 voiceprint store, so v1 → v1.5 is additive, not a refactor.

---

## Dependencies / Assumptions

- PipeWire is the audio server. PulseAudio-only systems are not a v1 target.
- WhisperX, pyannote.audio, and a local LLM via Ollama are installable on the user's box. v1 does not bundle these — it depends on them via the package manager. Acceptable: a one-time install/setup script that verifies and installs them.
- The user's machine has enough compute for Whisper + a local LLM. Performance bar: transcription at ≥ ~3× real-time on a consumer GPU; post-meeting summary in <30s for a 30-minute meeting.
- Channel-split audio capture (mic = "me", system = "everyone else") provides a strong diarization prior, so WhisperX does not start from open-world clustering on every call. This makes 2–6 person diarization tractable without persistent voiceprints.
- The user's downstream tools (Talos in this case, but also Obsidian vaults or future agents) will adapt to Furugura's markdown shape rather than the other way around.
- Granola-grade diarization quality is acceptable in v1 — speaker labels correct "most of the time" on 2–6 person calls, with manual rename for the rest. Not pyannote-DER-on-AMI accurate.

---

## Outstanding Questions

### Resolve Before Planning

(none — synthesis was confirmed)

### Deferred to Planning

- [Affects R1, R13] [Technical] What language / runtime does v1 ship in? Rust (matches future `wl-meetcap` direction; fast; single binary), Go (also single-binary; less mature audio bindings), Python (fastest to prototype; not single-binary-friendly), or shell + WhisperX CLI glue (smallest, weakest for the TUI live view in R7).
- [Affects R7] [Technical] What mechanism powers the `furu live` TUI? Native (e.g., Rust `ratatui`, Go `bubbletea`), or thinner — `tail -f` on a transcript file in `less` with `/` search, no real TUI required.
- [Affects R8] [Technical] How does `furu mark` discover the active meeting from a separate terminal? Env var, lockfile in `/run/user/$UID/`, named pipe, unix socket, all viable.
- [Affects R5] [Technical / Needs research] What is the smallest local LLM that produces Granola-grade summary on this user's hardware? Likely Qwen3-1.7B / Gemma3-4B class via Ollama, but worth a side-by-side benchmark before committing a default.
- [Affects R6] [Technical] How does the v1 → v1.5 transition handle existing meetings? Do renamed labels in v1 markdown files retroactively populate the v1.5 voiceprint store, or is enrollment v1.5-onward only?
- [Affects R10] [Technical] What is the canonical attendee detection in v1? Best-effort frontmatter from a calendar event the user passes manually (e.g., `furu start --calendar-id ...`), parsed Meet/Zoom URL extraction, or just "empty in v1, populated by v1.5/v1.6"?
