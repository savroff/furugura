//! Parse the LLM's response into a structured `SummaryBlock`.
//!
//! The system prompt asks for exactly three H3 sections in order
//! (Decisions, Action items, Key points). We tolerate:
//! - Trailing colons in headings (`### Action Items:`)
//! - Casing drift (`### Action Items` vs `### Action items`)
//! - Extra whitespace, leading bullets like `*` or `-`
//! - Sections appearing in any order; missing sections are returned empty.
//!
//! We do NOT tolerate the model producing prose before the first H3 — any
//! pre-section prose is dropped.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SummaryBlock {
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
    pub key_points: Vec<String>,
}

impl SummaryBlock {
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
            && self.action_items.is_empty()
            && self.key_points.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Decisions,
    ActionItems,
    KeyPoints,
}

fn section_for_heading(line: &str) -> Option<SectionKind> {
    let body = line.trim().trim_start_matches('#').trim();
    let body = body.trim_end_matches(':').trim();
    let lower = body.to_ascii_lowercase();
    match lower.as_str() {
        "decisions" => Some(SectionKind::Decisions),
        "action items" | "action-items" => Some(SectionKind::ActionItems),
        "key points" | "keypoints" | "key-points" => Some(SectionKind::KeyPoints),
        _ => None,
    }
}

pub fn parse_summary(text: &str) -> SummaryBlock {
    let mut out = SummaryBlock::default();
    let mut current: Option<SectionKind> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') {
            // Known heading switches sections; unknown headings (stray H1/H2
            // the model emitted) are ignored — we keep accumulating into the
            // current section so a single weird line doesn't drop content.
            if let Some(k) = section_for_heading(line) {
                current = Some(k);
            }
            continue;
        }
        let Some(kind) = current else { continue };
        if line.is_empty() {
            continue;
        }
        let bullet = strip_bullet_prefix(line);
        if bullet.is_empty() {
            continue;
        }
        // The "_none_" placeholder a section may include — don't treat as content.
        if bullet.eq_ignore_ascii_case("_none_") || bullet.eq_ignore_ascii_case("none") {
            continue;
        }
        match kind {
            SectionKind::Decisions => out.decisions.push(bullet.to_string()),
            SectionKind::ActionItems => out.action_items.push(bullet.to_string()),
            SectionKind::KeyPoints => out.key_points.push(bullet.to_string()),
        }
    }
    out
}

/// Remove leading `-`, `*`, `+`, or `1.` markers and surrounding whitespace.
/// Preserves the rest of the bullet content, including a `[ ]` task marker
/// since GitHub task-list syntax is the action-item format.
fn strip_bullet_prefix(line: &str) -> &str {
    let line = line.trim_start();
    let after_marker = if let Some(rest) = line.strip_prefix("- ") {
        rest
    } else if let Some(rest) = line.strip_prefix("* ") {
        rest
    } else if let Some(rest) = line.strip_prefix("+ ") {
        rest
    } else if let Some(rest) = line.strip_prefix("• ") {
        rest
    } else if line.starts_with(|c: char| c.is_ascii_digit()) {
        // 1. or 1) styles
        line.trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches(['.', ')'])
            .trim_start()
    } else {
        line
    };
    after_marker.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_three_section_response() {
        let text = "\
### Decisions
- **Decision:** Adopt PipeWire as the v1 audio target.
- **Decision:** Defer voiceprint persistence to v1.5.

### Action items
- [ ] Nick — write the U5 lifecycle (due 2026-05-10)
- [ ] Sarah — design the markdown schema

### Key points
- WhisperX requires CUDA, so whisper.cpp + Vulkan was chosen.
- pyannote runs at finalize, not live.
";
        let s = parse_summary(text);
        assert_eq!(s.decisions.len(), 2);
        assert!(s.decisions[0].starts_with("**Decision:**"));
        assert_eq!(s.action_items.len(), 2);
        assert!(s.action_items[0].starts_with("[ ]"));
        assert_eq!(s.key_points.len(), 2);
    }

    #[test]
    fn tolerates_heading_drift() {
        let text = "\
### Decisions:
- foo

### Action Items
- [ ] bar

### Key Points:
- baz
";
        let s = parse_summary(text);
        assert_eq!(s.decisions, vec!["foo"]);
        assert_eq!(s.action_items, vec!["[ ] bar"]);
        assert_eq!(s.key_points, vec!["baz"]);
    }

    #[test]
    fn tolerates_section_reorder() {
        let text = "\
### Key points
- a

### Decisions
- b

### Action items
- c
";
        let s = parse_summary(text);
        assert_eq!(s.key_points, vec!["a"]);
        assert_eq!(s.decisions, vec!["b"]);
        assert_eq!(s.action_items, vec!["c"]);
    }

    #[test]
    fn drops_pre_section_prose() {
        let text = "\
Sure, here's your summary!

### Decisions
- Adopt thing
";
        let s = parse_summary(text);
        assert_eq!(s.decisions, vec!["Adopt thing"]);
        assert_eq!(s.action_items.len(), 0);
    }

    #[test]
    fn missing_sections_are_empty_not_errors() {
        let text = "\
### Decisions
- only this section
";
        let s = parse_summary(text);
        assert_eq!(s.decisions.len(), 1);
        assert!(s.action_items.is_empty());
        assert!(s.key_points.is_empty());
    }

    #[test]
    fn none_placeholder_is_dropped() {
        let text = "\
### Decisions
- _none_

### Action items
- none

### Key points
- real point
";
        let s = parse_summary(text);
        assert!(s.decisions.is_empty());
        assert!(s.action_items.is_empty());
        assert_eq!(s.key_points, vec!["real point"]);
    }

    #[test]
    fn supports_star_and_numbered_bullets() {
        let text = "\
### Decisions
* foo
1. bar
2) baz
";
        let s = parse_summary(text);
        assert_eq!(s.decisions, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn empty_text_is_empty_block() {
        assert!(parse_summary("").is_empty());
        assert!(parse_summary("\n\n\n").is_empty());
    }

    #[test]
    fn ignores_h1_h2_headings_after_section_starts() {
        // If the model adds spurious H1/H2 headings, they shouldn't be
        // treated as section markers, but they shouldn't crash parsing.
        let text = "\
### Decisions
- ok
## (re-heading)
- after
";
        let s = parse_summary(text);
        // Both bullets land in Decisions because the H2 isn't a known section.
        assert_eq!(s.decisions, vec!["ok", "after"]);
    }
}
