//! U10 integration tests.
//!
//! Pure-logic tests live next to the module (frontmatter rendering,
//! placeholder behavior, atomic write). This file exercises end-to-end
//! integration between U3 (transcript), U9 (summary), and U10's writer.

use chrono::{Local, TimeZone};
use furugura::audio::capture::CaptureQuality;
use furugura::output::{MeetingBody, MeetingFrontmatter, assemble, write_meeting_md};
use furugura::summarize::{DataEgressed, SummaryAudit, SummaryBlock, parse_summary};
use furugura::transcribe::LiveSegment;

fn make_fm() -> MeetingFrontmatter {
    MeetingFrontmatter {
        id: "2026-05-02-1430-team".to_string(),
        start_time: Local.with_ymd_and_hms(2026, 5, 2, 14, 30, 0).unwrap(),
        end_time: Local.with_ymd_and_hms(2026, 5, 2, 15, 2, 0).unwrap(),
        attendees: vec![],
        audio_retained: false,
        capture_quality: CaptureQuality::Clean,
        transcription_engine: "whisper.cpp:large-v3".to_string(),
        diarization_model: "pyannote-3.1".to_string(),
        summary_audit: SummaryAudit {
            provider: "local".to_string(),
            model: "ollama:gemma3:4b".to_string(),
            data_egressed: DataEgressed::None,
        },
        tags: vec!["meeting".to_string()],
    }
}

#[test]
fn end_to_end_summary_parse_then_assemble_then_write() {
    let llm_response = "\
### Decisions
- **Decision:** Adopt PipeWire as v1 audio target.

### Action items
- [ ] Nick — write U5 lifecycle (due 2026-05-10)

### Key points
- whisper.cpp + Vulkan covers Intel Arc.
";
    let summary = parse_summary(llm_response);
    let body = MeetingBody {
        title: "Team standup".to_string(),
        notes: "[00:00:00] kickoff".to_string(),
        summary,
        transcript: vec![LiveSegment {
            t: "00:00:01.000".into(),
            speaker: Some("SPEAKER_00".into()),
            text: "We should ship v1 by Friday.".into(),
            start: 1.0,
            end: 3.0,
        }],
    };
    let md = assemble(&make_fm(), &body);

    // Frontmatter parseable by `serde_yaml`-style readers? We don't pull
    // a YAML parser here; instead, structural assertions:
    assert!(md.starts_with("---\nfurugura_version: 1\n"));
    assert!(md.contains("\n---\n\n# Team standup\n"));
    assert!(md.contains("\n## Notes\n\n[00:00:00] kickoff\n"));
    assert!(md.contains("\n## Summary\n\n### Decisions"));
    assert!(md.contains("- **Decision:** Adopt PipeWire as v1 audio target."));
    assert!(md.contains("- [ ] Nick — write U5 lifecycle (due 2026-05-10)"));
    assert!(md.contains("\n## Transcript\n\n[00:00:01.000] **SPEAKER_00:** We should ship v1 by Friday."));

    let dir = tempfile::tempdir().unwrap();
    let written = write_meeting_md(dir.path(), &md).unwrap();
    let read = std::fs::read_to_string(&written).unwrap();
    assert_eq!(read, md);
}

#[test]
fn cloud_path_audit_fields_visible_in_frontmatter() {
    let mut fm = make_fm();
    fm.summary_audit = SummaryAudit {
        provider: "cloud:anthropic".to_string(),
        model: "anthropic:claude-opus-4-7".to_string(),
        data_egressed: DataEgressed::FullTranscript,
    };
    fm.attendees = vec!["alice@example.com".to_string(), "bob@example.com".to_string()];
    let body = MeetingBody {
        title: "Sales call".to_string(),
        notes: "".to_string(),
        summary: SummaryBlock::default(),
        transcript: vec![],
    };
    let md = assemble(&fm, &body);
    assert!(md.contains("summary_provider: \"cloud:anthropic\""));
    assert!(md.contains("data_egressed: full_transcript"));
    assert!(md.contains("alice@example.com"));
    assert!(md.contains("bob@example.com"));
}

#[test]
fn atomic_write_does_not_leave_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    let target_dir = dir.path().join("nested");
    let md = assemble(
        &make_fm(),
        &MeetingBody {
            title: "T".to_string(),
            notes: "".to_string(),
            summary: SummaryBlock::default(),
            transcript: vec![],
        },
    );
    write_meeting_md(&target_dir, &md).unwrap();

    let entries: Vec<_> = std::fs::read_dir(&target_dir).unwrap().collect();
    // Exactly one file: meeting.md. No leftover .tmpXXXX files.
    let names: Vec<String> = entries
        .iter()
        .map(|e| e.as_ref().unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["meeting.md"], "stray temp files: {names:?}");
}
