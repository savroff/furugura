---
date: 2026-05-02
status: v1 feature-complete; awaiting first real-meeting validation
version: 0.1.2
github: https://github.com/savroff/furugura
---

# Furugura v1 — status, gaps, and what's next

## Where things stand

Furugura v1 is feature-complete. The `furu` binary is installed at
`~/.local/bin/furu` (version 0.1.2), pushed to
[github.com/savroff/furugura](https://github.com/savroff/furugura), and
runnable end-to-end. Every implementation unit from the
[v1 plan](../plans/2026-05-02-001-feat-furugura-v1-plan.md) (U1–U10) ships.
207 tests pass; clippy `-D warnings` clean across all targets.

All external dependencies are installed and verified via `furu setup --check`:
whisper.cpp + Vulkan, pyannote.audio (via pipx), Ollama with `gemma3:4b`
pulled, ffmpeg, PipeWire, vulkan-tools. HF token enrolled at
`~/.config/furugura/hf_token` (mode 0600).

## What's been validated

- **Unit tests** — every module has tests covering pure-logic surface
  (segment merge, RTTM parsing, frontmatter rendering, consent gate
  decisions, ID slugification, drift math, JSONL round-trip, etc.).
- **Real PipeWire capture** — U2 integration test hits real `pw-record`,
  captures 3 seconds of stereo, validates a clean WAV. Passes with
  `cargo test --test audio_capture_test -- --ignored`.
- **End-to-end orchestration** — `furu start` → `furu mark` from another
  terminal → `furu stop` → finalize → `meeting.md` produced with valid
  frontmatter and timestamped notes. AE5 from the requirements doc covered.
- **Graceful degradation** — when whisper / pyannote / ollama are missing,
  finalize still produces a markdown file with placeholders, and the
  audit fields name what was skipped.
- **Lock conflict** — `flock` on a separate `lock` file (after the
  inode-orphan bug was caught and fixed); second `furu start` correctly
  returns EAGAIN with a clear message.
- **Token hygiene** — HF and Anthropic tokens stored mode 0600,
  env-passed (not CLI flag) so they don't leak via `/proc/<pid>/cmdline`.

## What's NOT yet validated

This is the gap. Now that all deps are installed, none of the
production behaviour has been exercised against a real call.

- **Full pipeline against a real meeting.** No real Zoom/Meet/Teams call
  has been recorded end-to-end. The unit/integration tests stop at the
  module boundary.
- **Diarization accuracy.** The plan's v1 ship gate is "≥80% segments
  correctly attributed on a 4-speaker fixture call" (U3 acceptance gate).
  No fixture exists; no measurement done.
- **Summary quality.** Gemma 3 4B is the default model; the prompt has
  never seen a real transcript. First run will likely need tuning.
- **Latency.** Finalize-pipeline target is "<30s for a 30-min meeting"
  on Vulkan. Untested.
- **Audio routing edge cases.** Speaker-bleed warning fires correctly,
  but no real laptop-speakers-only meeting has been captured. Bluetooth
  profile switch mid-meeting (the recovery path) is untested.

## Next session — start here

1. **Run a real meeting smoke.** Get someone on a Zoom/Meet call for
   ~5 minutes:
   ```sh
   furu start --title "smoke"
   # ... talk ...
   furu stop
   ```
   Read `~/Meetings/<id>/meeting.md`. Questions to answer:
   - Did diarization actually run? (`diarization_model: pyannote-3.1`
     in frontmatter, not `none`)
   - Are speaker labels usable on 2-person calls?
   - Is the summary useful, or does the prompt need tuning?
   - Did finalize complete in reasonable time?
   - Does `capture_quality` reflect reality?

2. **Triage what surfaces.** Likely candidates and where to look:
   - Bad summary → iterate on `src/summarize/prompt.rs` (the system
     instructions block).
   - Bad diarization → consider whether the channel-split prior is
     working; check the RTTM sidecar.
   - High latency → switch batch model to `large-v3-q5_k` or smaller.
   - Audio quality issues → check the always-written WAV in tmpfs
     before it gets unlinked, or run with `--keep-audio` to inspect later.

3. **Document findings in a follow-up status doc.**

## Follow-ups (not blocking real-meeting smoke)

- **GitHub Actions CI** — `cargo test` + `cargo clippy -D warnings` on
  every push. Free regression coverage. ~20 min of work.
- **GitHub Release for v0.1.2** — tag exists, no Release page with
  notes yet.
- **AUR `-bin` packaging** — wait until v1 stabilizes through real use.
- **v1.5 voiceprint persistence** — the most-leverage post-v1 work
  because it removes the per-meeting rename burden. RTTM sidecar is
  already persisted in v1 specifically to enable retroactive enrollment.

## Decisions worth remembering

- **Lockfile is separate from the state JSON.** flock on
  `active-meeting.json` directly was being orphaned by the atomic
  rename in `write_active_meeting`, allowing concurrent meetings to
  start. Lock now lives at `$XDG_RUNTIME_DIR/furugura/lock`, state at
  `active-meeting.json`. Caught by the lock-conflict smoke test before
  it shipped to anyone.
- **`pyannote_runner` is a Bash wrapper, not a Python script.** pipx
  isolates pyannote in its own venv at
  `~/.local/share/pipx/venvs/pyannote-audio/bin/python`, invisible to
  the system python the runner originally assumed. The wrapper probes
  the pipx venv first, falls back to system python, then execs an
  inline diarization driver.
- **README softened for trademark hygiene.** The original attribution
  to "Calbee's Frugra cereal" was removed; the name "Furugura" stays.
  Reasoning: trademark (not copyright) is the actual exposure, and
  removing the explicit brand reference takes the most direct
  similarity signal off the table without renaming. Documented in
  commit `5034bb9`. Full rename remains a one-day refactor if the
  project picks up audience.
- **README voice is conversational.** Opens with the pain (Granola is
  macOS-only), shows the meeting workflow before any install commands,
  uses "What you saw / What's probably wrong / What to try" headers
  instead of formal docs language. The previous "feature manifest"
  voice was deliberately replaced.

## Repo state at session end

```
v0.1.2  (latest tag, on origin/main)
├── 5034bb9 docs: soften README name attribution
├── ddae57a docs: rewrite README in a more conversational voice
├── c3e3822 fix: pyannote_runner + setup probe handle pipx-venv installs (0.1.2)
├── fffe274 fix(setup): use yay/pacman/python-pipx hints to match Arch reality
├── f228b17 docs: rewrite README as the v1 user front door
├── dcfe912 feat: complete v1 — setup, lifecycle, mark, live TUI, list/edit, summary, markdown
├── b69a5a2 feat: scaffold v1 foundation, audio capture, and transcription pipeline
└── e1f115b chore: initialize Furugura project with planning docs
```

Working tree clean. All v1 work committed and pushed. CI not yet set up.
