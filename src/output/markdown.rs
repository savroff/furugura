//! Markdown assembler for `meeting.md`.
//!
//! The schema is the v1 "format spec" for downstream consumers (Talos,
//! Obsidian, agentic readers). It is deliberately stable: snake_case
//! frontmatter keys, `furugura_version: 1` discriminator, three H2 body
//! sections in a fixed order, three H3 summary subsections in a fixed
//! order, action items in GitHub task-list syntax, decisions prefixed
//! with `**Decision:**`, transcript turns in `[HH:MM:SS.mmm] **SPEAKER_NN:** text`
//! form.
//!
//! Empty content gets explicit placeholders so the schema shape is
//! preserved regardless of input.

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::audio::capture::CaptureQuality;
use crate::summarize::{SummaryAudit, SummaryBlock};
use crate::transcribe::LiveSegment;

pub const FURUGURA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct MeetingFrontmatter {
    pub id: String,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub attendees: Vec<String>,
    pub audio_retained: bool,
    pub capture_quality: CaptureQuality,
    /// e.g. `whisper.cpp:large-v3`
    pub transcription_engine: String,
    /// e.g. `pyannote-3.1`
    pub diarization_model: String,
    pub summary_audit: SummaryAudit,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MeetingBody {
    pub title: String,
    pub notes: String,
    pub summary: SummaryBlock,
    pub transcript: Vec<LiveSegment>,
}

/// Assemble the full markdown text. Pure function — no I/O.
pub fn assemble(fm: &MeetingFrontmatter, body: &MeetingBody) -> String {
    let mut out = String::with_capacity(8192);
    write_frontmatter(&mut out, fm);
    out.push('\n');
    out.push_str(&format!("# {}\n\n", body.title.trim()));
    write_notes(&mut out, &body.notes);
    write_summary(&mut out, &body.summary);
    write_transcript(&mut out, &body.transcript);
    out
}

/// Atomic-write `meeting.md` into `meeting_dir`. Returns the final path.
pub fn write_meeting_md(meeting_dir: &Path, content: &str) -> Result<PathBuf> {
    crate::paths::ensure_dir(meeting_dir)?;
    let target = meeting_dir.join("meeting.md");
    let mut tmp = tempfile::NamedTempFile::new_in(meeting_dir)
        .with_context(|| format!("could not create temp file in {}", meeting_dir.display()))?;
    tmp.write_all(content.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(&target).map_err(|e| {
        anyhow::anyhow!(
            "could not rename temp file to {}: {}",
            target.display(),
            e.error,
        )
    })?;
    Ok(target)
}

// ─────────────────────────────────────────────────────────────────────────
// Internal: frontmatter

fn write_frontmatter(out: &mut String, fm: &MeetingFrontmatter) {
    let duration_minutes = (fm.end_time - fm.start_time).num_seconds().max(0) / 60;
    out.push_str("---\n");
    out.push_str(&format!("furugura_version: {FURUGURA_VERSION}\n"));
    out.push_str(&format!("id: {}\n", yaml_string(&fm.id)));
    out.push_str(&format!(
        "date: {}\n",
        fm.start_time.format("%Y-%m-%d"),
    ));
    out.push_str(&format!(
        "start_time: {}\n",
        fm.start_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    ));
    out.push_str(&format!(
        "end_time: {}\n",
        fm.end_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    ));
    out.push_str(&format!("duration_minutes: {duration_minutes}\n"));
    out.push_str(&format!("attendees: {}\n", yaml_list(&fm.attendees)));
    out.push_str(&format!("audio_retained: {}\n", fm.audio_retained));
    out.push_str(&format!(
        "capture_quality: {}\n",
        match fm.capture_quality {
            CaptureQuality::Clean => "clean",
            CaptureQuality::Degraded => "degraded",
        },
    ));
    out.push_str(&format!(
        "transcription_engine: {}\n",
        yaml_string(&fm.transcription_engine),
    ));
    out.push_str(&format!(
        "diarization_model: {}\n",
        yaml_string(&fm.diarization_model),
    ));
    out.push_str(&format!(
        "summary_provider: {}\n",
        yaml_string(&fm.summary_audit.provider),
    ));
    out.push_str(&format!(
        "summary_model: {}\n",
        yaml_string(&fm.summary_audit.model),
    ));
    out.push_str(&format!(
        "data_egressed: {}\n",
        fm.summary_audit.data_egressed.as_str(),
    ));
    out.push_str(&format!("tags: {}\n", yaml_list(&fm.tags)));
    out.push_str("---\n");
}

/// Quote a YAML string only when needed (simple ASCII identifiers go
/// through bare). The frontmatter values we emit (id slugs, model names,
/// timestamps) are mostly safe; quote anything with whitespace, colons,
/// or YAML special characters.
fn yaml_string(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quoting = s.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                ':' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`'
            )
    }) || s.starts_with(['-', '?'])
        || s.eq_ignore_ascii_case("true")
        || s.eq_ignore_ascii_case("false")
        || s.eq_ignore_ascii_case("null")
        || s.eq_ignore_ascii_case("yes")
        || s.eq_ignore_ascii_case("no")
        || s.parse::<f64>().is_ok();
    if needs_quoting {
        // Use double-quoted form with backslash escapes for `\` and `"`.
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn yaml_list(items: &[String]) -> String {
    if items.is_empty() {
        "[]".to_string()
    } else {
        let parts: Vec<String> = items.iter().map(|s| yaml_string(s)).collect();
        format!("[{}]", parts.join(", "))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Internal: body

fn write_notes(out: &mut String, notes: &str) {
    out.push_str("## Notes\n\n");
    let trimmed = notes.trim();
    if trimmed.is_empty() {
        out.push_str("_(no notes captured)_\n\n");
    } else {
        out.push_str(trimmed);
        out.push_str("\n\n");
    }
}

fn write_summary(out: &mut String, s: &SummaryBlock) {
    out.push_str("## Summary\n\n");
    write_summary_section(out, "Decisions", "_no decisions_", &s.decisions);
    write_summary_section(out, "Action items", "_no action items_", &s.action_items);
    write_summary_section(out, "Key points", "_no key points_", &s.key_points);
}

fn write_summary_section(out: &mut String, heading: &str, empty: &str, items: &[String]) {
    out.push_str(&format!("### {heading}\n\n"));
    if items.is_empty() {
        out.push_str(&format!("- {empty}\n\n"));
    } else {
        for item in items {
            // Items from `parse_summary` already lack their bullet prefix;
            // re-add a single leading dash. If the item already starts
            // with `[ ]` (action item), preserve that.
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
        out.push('\n');
    }
}

fn write_transcript(out: &mut String, segments: &[LiveSegment]) {
    out.push_str("## Transcript\n\n");
    if segments.is_empty() {
        out.push_str("_(no transcript)_\n");
        return;
    }
    for seg in segments {
        let speaker = seg.speaker.as_deref().unwrap_or("SPEAKER_??");
        out.push_str(&format!("[{}] **{speaker}:** {}\n", seg.t, seg.text.trim()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fm() -> MeetingFrontmatter {
        let start = Local.with_ymd_and_hms(2026, 5, 2, 14, 30, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 5, 2, 15, 2, 0).unwrap();
        MeetingFrontmatter {
            id: "2026-05-02-1430-team-standup".to_string(),
            start_time: start,
            end_time: end,
            attendees: vec![],
            audio_retained: false,
            capture_quality: CaptureQuality::Clean,
            transcription_engine: "whisper.cpp:large-v3".to_string(),
            diarization_model: "pyannote-3.1".to_string(),
            summary_audit: SummaryAudit {
                provider: "local".to_string(),
                model: "ollama:gemma3:4b".to_string(),
                data_egressed: crate::summarize::DataEgressed::None,
            },
            tags: vec!["meeting".to_string()],
        }
    }

    fn body() -> MeetingBody {
        MeetingBody {
            title: "Team Standup".to_string(),
            notes: "[00:00:00] kickoff\n[00:01:23] Sarah's blocker".to_string(),
            summary: SummaryBlock {
                decisions: vec!["**Decision:** Ship v1 by Friday.".to_string()],
                action_items: vec!["[ ] Sarah — write README (due 2026-05-09)".to_string()],
                key_points: vec!["Pipeline runs locally by default.".to_string()],
            },
            transcript: vec![
                LiveSegment {
                    t: "00:00:01.000".into(),
                    speaker: Some("SPEAKER_00".into()),
                    text: "Hello.".into(),
                    start: 1.0,
                    end: 1.5,
                },
                LiveSegment {
                    t: "00:00:02.500".into(),
                    speaker: Some("SPEAKER_01".into()),
                    text: "Hi.".into(),
                    start: 2.5,
                    end: 3.0,
                },
            ],
        }
    }

    #[test]
    fn frontmatter_has_required_keys_in_order() {
        let md = assemble(&fm(), &body());
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines[0], "---");
        assert_eq!(lines[1], "furugura_version: 1");
        assert!(lines[2].starts_with("id: 2026-05-02-1430-team-standup"));
        assert!(lines[3].starts_with("date: 2026-05-02"));
        assert!(lines[4].starts_with("start_time: 2026-05-02T14:30:00"));
        assert!(lines[5].starts_with("end_time: 2026-05-02T15:02:00"));
        assert_eq!(lines[6], "duration_minutes: 32");
        assert_eq!(lines[7], "attendees: []");
        assert_eq!(lines[8], "audio_retained: false");
        assert_eq!(lines[9], "capture_quality: clean");
        // Closing fence is line N: find it.
        let close = md.match_indices("\n---\n").next().unwrap().0;
        let after = &md[close + 5..];
        assert!(after.starts_with("\n# Team Standup"));
    }

    #[test]
    fn empty_notes_render_placeholder() {
        let mut b = body();
        b.notes = "   ".to_string();
        let md = assemble(&fm(), &b);
        assert!(md.contains("## Notes\n\n_(no notes captured)_"));
    }

    #[test]
    fn empty_summary_sections_render_placeholders() {
        let mut b = body();
        b.summary = SummaryBlock::default();
        let md = assemble(&fm(), &b);
        assert!(md.contains("- _no decisions_"));
        assert!(md.contains("- _no action items_"));
        assert!(md.contains("- _no key points_"));
    }

    #[test]
    fn empty_transcript_renders_placeholder() {
        let mut b = body();
        b.transcript.clear();
        let md = assemble(&fm(), &b);
        assert!(md.contains("## Transcript\n\n_(no transcript)_"));
    }

    #[test]
    fn transcript_uses_speaker_bold_format() {
        let md = assemble(&fm(), &body());
        assert!(md.contains("[00:00:01.000] **SPEAKER_00:** Hello."));
        assert!(md.contains("[00:00:02.500] **SPEAKER_01:** Hi."));
    }

    #[test]
    fn missing_speaker_renders_placeholder() {
        let mut b = body();
        b.transcript[0].speaker = None;
        let md = assemble(&fm(), &b);
        assert!(md.contains("[00:00:01.000] **SPEAKER_??:** Hello."));
    }

    #[test]
    fn cloud_audit_fields_surface_in_frontmatter() {
        let mut f = fm();
        f.summary_audit = SummaryAudit {
            provider: "cloud:anthropic".to_string(),
            model: "anthropic:claude-opus-4-7".to_string(),
            data_egressed: crate::summarize::DataEgressed::FullTranscript,
        };
        let md = assemble(&f, &body());
        // cloud:anthropic contains a colon, so it must be quoted.
        assert!(md.contains("summary_provider: \"cloud:anthropic\""));
        assert!(md.contains("summary_model: \"anthropic:claude-opus-4-7\""));
        assert!(md.contains("data_egressed: full_transcript"));
    }

    #[test]
    fn capture_quality_degraded_renders() {
        let mut f = fm();
        f.capture_quality = CaptureQuality::Degraded;
        let md = assemble(&f, &body());
        assert!(md.contains("capture_quality: degraded"));
    }

    #[test]
    fn attendees_render_quoted_when_containing_special_chars() {
        let mut f = fm();
        f.attendees = vec!["alice@example.com".to_string()];
        let md = assemble(&f, &body());
        assert!(md.contains("attendees: [alice@example.com]") || md.contains("attendees: [\"alice@example.com\"]"));
    }

    #[test]
    fn yaml_string_quoting_rules() {
        assert_eq!(yaml_string("simple"), "simple");
        assert_eq!(yaml_string("with space"), "\"with space\"");
        assert_eq!(yaml_string("colon:value"), "\"colon:value\"");
        assert_eq!(yaml_string("123"), "\"123\"");
        assert_eq!(yaml_string("true"), "\"true\"");
        assert_eq!(yaml_string(""), "\"\"");
        assert_eq!(yaml_string("with\"quote"), "\"with\\\"quote\"");
        assert_eq!(yaml_string("plain-id"), "plain-id"); // no leading dash → bare
        assert_eq!(yaml_string("-leading-dash"), "\"-leading-dash\"");
    }

    #[test]
    fn write_meeting_md_atomic_writes_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let target_dir = dir.path().join("2026-05-02-1430");
        let md = assemble(&fm(), &body());
        let written = write_meeting_md(&target_dir, &md).unwrap();
        assert_eq!(written.file_name().unwrap(), "meeting.md");
        let content = std::fs::read_to_string(&written).unwrap();
        assert_eq!(content, md);
    }

    #[test]
    fn write_meeting_md_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target_dir = dir.path().join("2026-05-02-1430");
        write_meeting_md(&target_dir, "old\n").unwrap();
        let md = assemble(&fm(), &body());
        let written = write_meeting_md(&target_dir, &md).unwrap();
        let content = std::fs::read_to_string(&written).unwrap();
        assert_eq!(content, md);
    }

    #[test]
    fn duration_minutes_is_floor_of_seconds() {
        let mut f = fm();
        // Set end - start = 32m 59s → 32 minutes (floor)
        f.end_time = f.start_time + chrono::Duration::seconds(32 * 60 + 59);
        let md = assemble(&f, &body());
        assert!(md.contains("duration_minutes: 32"));
    }
}
