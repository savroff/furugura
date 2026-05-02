# Furugura

CLI-only Linux meeting capture, transcription, and AI summary — a Granola-shaped tool for users who live in the terminal.

**フルグラ** (furugura) is a Japanese portmanteau of fruit + granola, popularized by Calbee's Frugra cereal. The framing: granola, but richer — an open-source remix shaped for the Linux power-user audience.

## Status

v1 in active development. See `docs/plans/2026-05-02-001-feat-furugura-v1-plan.md` for the implementation plan.

## What it does

- Captures mic + system audio simultaneously on PipeWire-based Linux (Omarchy / Hyprland is the dev target).
- Transcribes via [whisper.cpp](https://github.com/ggerganov/whisper.cpp) with Vulkan acceleration; diarizes via [pyannote.audio](https://github.com/pyannote/pyannote-audio).
- Summarizes locally via [Ollama](https://github.com/ollama/ollama) by default; cloud LLM (Anthropic) is opt-in with an explicit consent gate.
- Outputs one Markdown file per meeting under `~/Meetings/<id>/` with structured frontmatter, AI summary, and full diarized transcript — readable by humans and ingestable by downstream tools (personal-knowledge pipelines, Obsidian vaults, agent toolchains).
- Live transcript view (`furu live`) and timestamped manual markers (`furu mark "..."`) during the meeting.
- Audio is held in volatile (tmpfs) storage during the meeting and discarded by default; opt-in retention via `--keep-audio`.

## Documentation

- [v1 requirements](docs/brainstorms/2026-05-02-furugura-v1-requirements.md) — what we're building and why.
- [v1 implementation plan](docs/plans/2026-05-02-001-feat-furugura-v1-plan.md) — how we're building it.

## License

MIT. See `LICENSE`.
