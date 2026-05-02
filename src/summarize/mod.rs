//! Post-meeting summarization.
//!
//! Two providers, mutually exclusive per meeting:
//!   - `local` — Ollama on loopback (default), no transcript leaves the machine.
//!   - `cloud:anthropic` — opt-in, gated by an interactive consent prompt
//!     and an attendee guard.
//!
//! All providers produce the same `SummaryBlock` (three H3 sections) and
//! the same `SummaryAudit` record (frontmatter fields for U10).

pub mod anthropic;
pub mod ollama;
pub mod parse;
pub mod prompt;

pub use parse::{SummaryBlock, parse_summary};
pub use prompt::{PromptInputs, assemble_prompt};

/// Whether the consent gate must run before a cloud summary fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    /// Proceed without prompting (local; or cloud with `--yes`).
    Proceed,
    /// Cloud path; the orchestrator must show the interactive notice and
    /// wait for the user to press Enter (Ctrl-C aborts).
    NeedsInteractivePrompt,
    /// Refuse outright. Cloud path with named attendees and no
    /// `--i-have-consent` flag — third-party data egress requires explicit
    /// acknowledgment.
    RefuseAttendees,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Local,
    CloudAnthropic,
}

#[derive(Debug, Clone, Copy)]
pub struct ConsentInputs<'a> {
    pub provider: ProviderKind,
    pub attendees: &'a [String],
    pub yes_flag: bool,
    pub attendee_consent_flag: bool,
}

pub fn evaluate_consent(inputs: ConsentInputs<'_>) -> ConsentDecision {
    match inputs.provider {
        ProviderKind::Local => ConsentDecision::Proceed,
        ProviderKind::CloudAnthropic => {
            if !inputs.attendees.is_empty() && !inputs.attendee_consent_flag {
                return ConsentDecision::RefuseAttendees;
            }
            if inputs.yes_flag {
                ConsentDecision::Proceed
            } else {
                ConsentDecision::NeedsInteractivePrompt
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(
        provider: ProviderKind,
        attendees: &'a [String],
        yes: bool,
        consent: bool,
    ) -> ConsentInputs<'a> {
        ConsentInputs {
            provider,
            attendees,
            yes_flag: yes,
            attendee_consent_flag: consent,
        }
    }

    #[test]
    fn local_always_proceeds() {
        let attendees = vec!["alice@example.com".to_string()];
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::Local, &attendees, false, false)),
            ConsentDecision::Proceed,
        );
    }

    #[test]
    fn cloud_with_no_attendees_needs_prompt_or_yes() {
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &[], false, false)),
            ConsentDecision::NeedsInteractivePrompt,
        );
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &[], true, false)),
            ConsentDecision::Proceed,
        );
    }

    #[test]
    fn cloud_with_attendees_refuses_without_explicit_consent() {
        let a = vec!["alice@example.com".to_string()];
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &a, false, false)),
            ConsentDecision::RefuseAttendees,
        );
        // Even with --yes, refusal stands until --i-have-consent is set.
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &a, true, false)),
            ConsentDecision::RefuseAttendees,
        );
    }

    #[test]
    fn cloud_with_attendees_and_consent_still_needs_prompt_unless_yes() {
        let a = vec!["alice@example.com".to_string()];
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &a, false, true)),
            ConsentDecision::NeedsInteractivePrompt,
        );
        assert_eq!(
            evaluate_consent(inputs(ProviderKind::CloudAnthropic, &a, true, true)),
            ConsentDecision::Proceed,
        );
    }

    #[test]
    fn data_egressed_strs() {
        assert_eq!(DataEgressed::None.as_str(), "none");
        assert_eq!(DataEgressed::FullTranscript.as_str(), "full_transcript");
    }
}

/// Audit fields written into meeting.md frontmatter so the privacy
/// posture is observable on disk (R10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryAudit {
    /// `local` or `cloud:<provider>`.
    pub provider: String,
    /// Model identifier, e.g. `ollama:gemma3:4b` or `anthropic:claude-opus-4-7`.
    pub model: String,
    /// What left this machine.
    pub data_egressed: DataEgressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataEgressed {
    None,
    FullTranscript,
}

impl DataEgressed {
    pub fn as_str(self) -> &'static str {
        match self {
            DataEgressed::None => "none",
            DataEgressed::FullTranscript => "full_transcript",
        }
    }
}
