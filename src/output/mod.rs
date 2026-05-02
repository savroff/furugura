//! Per-meeting markdown finalization.
//!
//! Produces `~/Meetings/<id>/meeting.md`: YAML frontmatter (audit fields
//! observable for downstream consumers) + three H2 body sections
//! (`## Notes`, `## Summary`, `## Transcript`). Atomic write via
//! `tempfile`'s temp-then-persist pattern so partial writes never appear.
//!
//! Anything besides the markdown file (RTTM sidecar copy, audio
//! preservation, runtime dir cleanup) is the lifecycle layer's job (U5).

pub mod markdown;

pub use markdown::{
    MeetingBody, MeetingFrontmatter, assemble, write_meeting_md,
};
