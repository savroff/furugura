//! Meeting ID generation.
//!
//! Format: `YYYY-MM-DD-HHMM[-<slug>]`. The slug is optional and derived
//! from `--title` if provided. The ID is the directory name under both
//! `$XDG_RUNTIME_DIR/furugura/<id>/` (during the meeting) and the
//! configured output dir (after finalize).

use chrono::{DateTime, Local};

pub fn make_id(start: DateTime<Local>, title: Option<&str>) -> String {
    let stamp = start.format("%Y-%m-%d-%H%M");
    match title.and_then(|t| {
        let slug = slugify_title(t);
        if slug.is_empty() { None } else { Some(slug) }
    }) {
        Some(slug) => format!("{stamp}-{slug}"),
        None => format!("{stamp}"),
    }
}

/// Slugify rules:
/// - lowercase
/// - alphanumerics and `-` allowed; everything else → `-`
/// - collapse runs of `-`
/// - trim leading/trailing `-`
/// - cap at 60 characters (keeps directory names sane on every fs)
pub fn slugify_title(title: &str) -> String {
    let mut buf = String::with_capacity(title.len());
    let mut last_was_dash = true;
    for c in title.chars() {
        let lc = c.to_ascii_lowercase();
        let is_safe = lc.is_ascii_alphanumeric();
        if is_safe {
            buf.push(lc);
            last_was_dash = false;
        } else if !last_was_dash {
            buf.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = buf.trim_matches('-');
    let mut out = trimmed.to_string();
    if out.len() > 60 {
        out.truncate(60);
        out = out.trim_end_matches('-').to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn moment() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 5, 2, 14, 30, 0).unwrap()
    }

    #[test]
    fn no_title_yields_timestamp_only() {
        assert_eq!(make_id(moment(), None), "2026-05-02-1430");
        assert_eq!(make_id(moment(), Some("")), "2026-05-02-1430");
        assert_eq!(make_id(moment(), Some("   ")), "2026-05-02-1430");
    }

    #[test]
    fn title_appended_as_slug() {
        assert_eq!(
            make_id(moment(), Some("Team Standup")),
            "2026-05-02-1430-team-standup",
        );
    }

    #[test]
    fn slug_lowercase_alnum_dash_only() {
        assert_eq!(slugify_title("Team Standup!"), "team-standup");
        assert_eq!(slugify_title("Q3 Planning / Sales"), "q3-planning-sales");
        assert_eq!(slugify_title("---weird---title---"), "weird-title");
        assert_eq!(slugify_title("multi   spaces"), "multi-spaces");
    }

    #[test]
    fn slug_handles_unicode_by_dropping() {
        // Non-ASCII chars become dashes (with collapse).
        assert_eq!(slugify_title("café meeting"), "caf-meeting");
        assert_eq!(slugify_title("会議 standup"), "standup");
    }

    #[test]
    fn slug_caps_at_60_chars_without_trailing_dash() {
        let very_long = "a".repeat(120);
        assert!(slugify_title(&very_long).len() <= 60);
        let mixed = format!("{} {}", "x".repeat(58), "yyyyyy");
        let s = slugify_title(&mixed);
        assert!(s.len() <= 60);
        assert!(!s.ends_with('-'));
    }

    #[test]
    fn slug_empty_for_pure_punctuation() {
        assert_eq!(slugify_title("///---!!!"), "");
    }
}
