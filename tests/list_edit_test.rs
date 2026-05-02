//! U8 integration tests.

use furugura::cli::edit::resolve_meeting_path;
use furugura::cli::list::{
    extract_frontmatter, parse_attendees, parse_frontmatter_lines, parse_id_date,
    parse_since, scan_meetings,
};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn end_to_end_scan_and_resolve() {
    let dir = tempdir().unwrap();
    for (id, ts) in [
        ("2026-05-02-1430-team", "2026-05-02T14:30:00-04:00"),
        ("2026-05-03-1000-q3-planning", "2026-05-03T10:00:00-04:00"),
    ] {
        let mdir = dir.path().join(id);
        std::fs::create_dir(&mdir).unwrap();
        std::fs::write(
            mdir.join("meeting.md"),
            format!(
                "---\nid: {id}\nstart_time: {ts}\nduration_minutes: 32\nattendees: [alice@x, bob@x]\n---\n",
            ),
        )
        .unwrap();
    }
    let summaries = scan_meetings(dir.path(), None).unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].id, "2026-05-03-1000-q3-planning");
    assert_eq!(summaries[0].duration_minutes, Some(32));
    assert_eq!(summaries[0].attendees.len(), 2);

    let pairs: Vec<(String, PathBuf)> = summaries
        .iter()
        .map(|s| (s.id.clone(), s.path.clone()))
        .collect();
    let p = resolve_meeting_path("1", &pairs).unwrap();
    assert!(p.ends_with("2026-05-03-1000-q3-planning/meeting.md"));

    let p2 = resolve_meeting_path("team", &pairs).unwrap();
    assert!(p2.ends_with("2026-05-02-1430-team/meeting.md"));
}

#[test]
fn frontmatter_parser_round_trip_with_actual_writer_output() {
    use chrono::{Local, TimeZone};
    use furugura::audio::capture::CaptureQuality;
    use furugura::output::{MeetingBody, MeetingFrontmatter, assemble};
    use furugura::summarize::{DataEgressed, SummaryAudit, SummaryBlock};
    use furugura::transcribe::LiveSegment;

    let fm = MeetingFrontmatter {
        id: "2026-05-02-1430-test".to_string(),
        start_time: Local.with_ymd_and_hms(2026, 5, 2, 14, 30, 0).unwrap(),
        end_time: Local.with_ymd_and_hms(2026, 5, 2, 15, 2, 0).unwrap(),
        attendees: vec!["alice@x.com".to_string(), "bob@y.com".to_string()],
        audio_retained: false,
        capture_quality: CaptureQuality::Clean,
        transcription_engine: "whisper.cpp:large-v3".to_string(),
        diarization_model: "pyannote-3.1".to_string(),
        summary_audit: SummaryAudit {
            provider: "cloud:anthropic".to_string(),
            model: "anthropic:claude-opus-4-7".to_string(),
            data_egressed: DataEgressed::FullTranscript,
        },
        tags: vec!["meeting".to_string()],
    };
    let body = MeetingBody {
        title: "T".to_string(),
        notes: "".to_string(),
        summary: SummaryBlock::default(),
        transcript: vec![LiveSegment {
            t: "00:00:01.000".into(),
            speaker: Some("SPEAKER_00".into()),
            text: "x".into(),
            start: 1.0,
            end: 2.0,
        }],
    };
    let md = assemble(&fm, &body);
    let block = extract_frontmatter(&md).unwrap();
    let kvs = parse_frontmatter_lines(block);

    let id = kvs.iter().find(|(k, _)| k == "id").unwrap().1.as_str();
    assert_eq!(id, "2026-05-02-1430-test");

    let provider = kvs
        .iter()
        .find(|(k, _)| k == "summary_provider")
        .unwrap()
        .1
        .as_str();
    assert_eq!(provider, "cloud:anthropic");

    let attendees_raw = kvs.iter().find(|(k, _)| k == "attendees").unwrap().1.as_str();
    let attendees = parse_attendees(attendees_raw);
    assert_eq!(attendees, vec!["alice@x.com", "bob@y.com"]);
}

#[test]
fn parse_id_date_for_filename_filter() {
    assert!(parse_id_date("2026-05-02-1430-team-standup").is_some());
    assert!(parse_id_date("not-a-meeting").is_none());
}

#[test]
fn parse_since_window() {
    let dt = parse_since("7d").unwrap();
    let now = chrono::Local::now();
    let diff = (now - dt).num_seconds();
    let week_secs = 7 * 24 * 60 * 60;
    assert!(
        (diff - week_secs as i64).abs() < 5,
        "7d should be ~7 days ago",
    );
}
