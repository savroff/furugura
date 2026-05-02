//! U9 integration tests.
//!
//! Pure-logic tests live next to their modules (prompt assembly, summary
//! parser, loopback check, consent evaluator, token-file safety). This
//! file exercises the integration surface — the public types from
//! `summarize::*` collectively form the interface U10 will read.

use furugura::summarize::{
    ConsentDecision, ConsentInputs, DataEgressed, ProviderKind, SummaryBlock,
    assemble_prompt, evaluate_consent, parse_summary,
};
use furugura::summarize::ollama::{
    DEFAULT_OLLAMA_ENDPOINT, endpoint_is_loopback, resolve_endpoint,
};
use furugura::summarize::prompt::PromptInputs;
use furugura::transcribe::LiveSegment;

#[test]
fn full_round_trip_prompt_then_response_then_block() {
    let segs = vec![
        LiveSegment {
            t: "00:00:01.000".into(),
            speaker: Some("SPEAKER_00".into()),
            text: "We should ship v1 by Friday.".into(),
            start: 1.0,
            end: 3.0,
        },
        LiveSegment {
            t: "00:00:04.000".into(),
            speaker: Some("SPEAKER_01".into()),
            text: "Agreed; Sarah will write the README.".into(),
            start: 4.0,
            end: 6.0,
        },
    ];
    let prompt = assemble_prompt(PromptInputs {
        user_notes: "[00:00:00] kickoff",
        transcript: &segs,
    });
    assert!(prompt.contains("kickoff"));
    assert!(prompt.contains("ship v1 by Friday"));

    // Simulate the LLM's response.
    let model_output = "\
### Decisions
- **Decision:** Ship v1 by Friday.

### Action items
- [ ] Sarah — write the README.

### Key points
- Two participants discussed v1 timing.
";
    let block: SummaryBlock = parse_summary(model_output);
    assert_eq!(block.decisions.len(), 1);
    assert_eq!(block.action_items.len(), 1);
    assert_eq!(block.key_points.len(), 1);
    assert!(block.decisions[0].contains("Ship v1 by Friday"));
    assert!(block.action_items[0].contains("Sarah"));
}

#[test]
fn local_provider_proceeds_without_attendee_guard() {
    let attendees = vec!["a@x".to_string(), "b@x".to_string()];
    let d = evaluate_consent(ConsentInputs {
        provider: ProviderKind::Local,
        attendees: &attendees,
        yes_flag: false,
        attendee_consent_flag: false,
    });
    assert_eq!(d, ConsentDecision::Proceed);
}

#[test]
fn cloud_with_attendees_blocks_until_consent_flag() {
    let attendees = vec!["a@x".to_string()];
    let blocked = evaluate_consent(ConsentInputs {
        provider: ProviderKind::CloudAnthropic,
        attendees: &attendees,
        yes_flag: true,
        attendee_consent_flag: false,
    });
    assert_eq!(blocked, ConsentDecision::RefuseAttendees);

    let unblocked = evaluate_consent(ConsentInputs {
        provider: ProviderKind::CloudAnthropic,
        attendees: &attendees,
        yes_flag: true,
        attendee_consent_flag: true,
    });
    assert_eq!(unblocked, ConsentDecision::Proceed);
}

#[test]
fn ollama_loopback_warning_suppressed_for_localhost_default() {
    assert!(endpoint_is_loopback(DEFAULT_OLLAMA_ENDPOINT));
}

#[test]
fn ollama_remote_endpoint_warns() {
    let resolved = resolve_endpoint(Some("https://gpu.example.com:11434"), None);
    assert!(!endpoint_is_loopback(&resolved));
}

#[test]
fn data_egressed_audit_strings() {
    // U10 will write these directly into the markdown frontmatter.
    assert_eq!(DataEgressed::None.as_str(), "none");
    assert_eq!(DataEgressed::FullTranscript.as_str(), "full_transcript");
}
