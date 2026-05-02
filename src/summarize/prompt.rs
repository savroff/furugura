//! Deterministic prompt assembly for the summarization pass.
//!
//! Both providers (Ollama, Anthropic) consume the same prompt. The system
//! instructions ask for exactly three H3 sections in a stable order, and
//! action items in GitHub task-list syntax. The parser (`parse.rs`)
//! tolerates minor format drift around this.

use crate::transcribe::LiveSegment;

pub const SYSTEM_INSTRUCTIONS: &str = "\
You are a meeting note-taker. Read the transcript and the user's manual notes, then output a summary.

Output exactly three H3 sections, in this order, even if a section is empty:
### Decisions
### Action items
### Key points

Each section is a bullet list. If a section has no content, write a single bullet: `- _none_`.
Action items use GitHub task-list syntax: `- [ ] <name> — <task> (due <YYYY-MM-DD>)` when a name or due date is mentioned; otherwise `- [ ] <task>`.
Decisions begin with `- **Decision:**` followed by a single sentence.
Do not add any other H1, H2, or H3 headings. Do not add prose outside the bullets.
";

pub struct PromptInputs<'a> {
    pub user_notes: &'a str,
    pub transcript: &'a [LiveSegment],
}

pub fn assemble_prompt(inputs: PromptInputs<'_>) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(SYSTEM_INSTRUCTIONS);
    out.push_str("\n---\n\n## Manual notes\n\n");
    if inputs.user_notes.trim().is_empty() {
        out.push_str("_(no manual notes)_\n");
    } else {
        out.push_str(inputs.user_notes.trim_end());
        out.push('\n');
    }

    out.push_str("\n## Transcript\n\n");
    if inputs.transcript.is_empty() {
        out.push_str("_(no transcript)_\n");
    } else {
        for seg in inputs.transcript {
            let speaker = seg.speaker.as_deref().unwrap_or("SPEAKER_??");
            // The format mirrors what U10 will write in the final markdown,
            // so summarizers see the same shape they will be cited against.
            out.push_str(&format!(
                "[{}] **{speaker}:** {}\n",
                seg.t,
                seg.text.trim(),
            ));
        }
    }
    out.push_str("\nProduce the three H3 sections now.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, text: &str, speaker: Option<&str>) -> LiveSegment {
        LiveSegment {
            t: crate::transcribe::jsonl::format_timestamp(start),
            speaker: speaker.map(String::from),
            text: text.into(),
            start,
            end: start + 1.0,
        }
    }

    #[test]
    fn includes_system_instructions_and_three_section_contract() {
        let p = assemble_prompt(PromptInputs {
            user_notes: "",
            transcript: &[],
        });
        assert!(p.contains("### Decisions"));
        assert!(p.contains("### Action items"));
        assert!(p.contains("### Key points"));
    }

    #[test]
    fn assembles_notes_and_transcript() {
        let segs = vec![
            seg(0.0, "Hello.", Some("SPEAKER_00")),
            seg(2.0, "Hi.", Some("SPEAKER_01")),
        ];
        let p = assemble_prompt(PromptInputs {
            user_notes: "[00:01:00] follow up Sarah",
            transcript: &segs,
        });
        assert!(p.contains("[00:01:00] follow up Sarah"));
        assert!(p.contains("[00:00:00.000] **SPEAKER_00:** Hello."));
        assert!(p.contains("[00:00:02.000] **SPEAKER_01:** Hi."));
    }

    #[test]
    fn missing_speaker_renders_placeholder() {
        let segs = vec![seg(1.0, "test", None)];
        let p = assemble_prompt(PromptInputs {
            user_notes: "",
            transcript: &segs,
        });
        assert!(p.contains("**SPEAKER_??:**"));
    }

    #[test]
    fn empty_inputs_produce_placeholders() {
        let p = assemble_prompt(PromptInputs {
            user_notes: "",
            transcript: &[],
        });
        assert!(p.contains("_(no manual notes)_"));
        assert!(p.contains("_(no transcript)_"));
    }

    #[test]
    fn whitespace_only_notes_count_as_empty() {
        let p = assemble_prompt(PromptInputs {
            user_notes: "   \n\n\t",
            transcript: &[],
        });
        assert!(p.contains("_(no manual notes)_"));
    }
}
