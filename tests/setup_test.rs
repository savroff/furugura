//! U4 integration tests for `furu setup`.
//!
//! Pure-logic unit tests live next to the module (token validation,
//! mode-0600 file writing, dep-spec well-formedness). This file
//! exercises `run_dep_checks` against the real subprocess probes —
//! the assertion is structural, not "all deps installed", because the
//! test environment may not have whisper.cpp / pyannote / etc.

use furugura::cli::setup::{DepReport, run_dep_checks, validate_hf_token};

#[tokio::test]
async fn run_dep_checks_returns_one_entry_per_dep() {
    let report: Vec<DepReport> = run_dep_checks().await;
    // The plan lists 9 dependencies; this test catches accidental dropouts.
    assert_eq!(report.len(), 9, "report length drift: {report:?}");

    // Every entry has a non-empty name and hint.
    for r in &report {
        assert!(!r.name.is_empty());
        assert!(!r.hint.is_empty());
    }

    // pactl is on every PipeWire/PulseAudio system the project targets.
    let pactl = report.iter().find(|r| r.name == "pactl").unwrap();
    if !pactl.ok {
        eprintln!(
            "note: pactl not detected — test environment is unusual but \
             the structural assertion still holds",
        );
    }
}

#[test]
fn validate_token_unit_smoke() {
    // Direct re-export of the inner unit test surface; ensures the
    // public symbol is reachable from a downstream consumer.
    assert!(validate_hf_token("hf_xxxxxxxxxxxxxxx").is_ok());
    assert!(validate_hf_token("").is_err());
}
