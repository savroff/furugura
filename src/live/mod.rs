//! Live transcript view (`furu live`).
//!
//! Reads `transcript.live.jsonl` from the active meeting, renders it in a
//! Ratatui viewport with scrollback and `/`-substring search, and updates
//! every ~250 ms while the meeting is `capturing`.

pub mod tui;

pub use tui::{App, AppEvent, KeyAction, dispatch_key};
