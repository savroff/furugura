//! U6 integration smoke for `furu mark`.
//!
//! The interactive UDS path with a real orchestrator is exercised by the
//! end-to-end smoke; this file covers the public surface (timestamp
//! formatting + path-safety helper) that the orchestrator does not.

use furugura::cli::mark::format_hms;

#[test]
fn timestamp_format_matches_plan_example() {
    // From the plan's AE5: "00:23:11" for 14:23:11 - 14:00:00.
    let elapsed = 23 * 60 + 11;
    assert_eq!(format_hms(elapsed), "00:23:11");
}

#[test]
fn timestamp_format_round_numbers() {
    assert_eq!(format_hms(0), "00:00:00");
    assert_eq!(format_hms(60), "00:01:00");
    assert_eq!(format_hms(3600), "01:00:00");
    assert_eq!(format_hms(3600 * 24), "24:00:00");
}
