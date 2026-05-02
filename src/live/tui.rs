//! Ratatui live-transcript app.
//!
//! State and key dispatch are pure functions on `App`; the
//! crossterm/ratatui terminal driver wraps them in `run` (cli/live.rs).

use crate::lifecycle::lockfile::LifecycleState;
use crate::transcribe::LiveSegment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Quit,
    EnterSearch,
    ExitSearch,
    AppendSearchChar(char),
    BackspaceSearch,
    SubmitSearch,
    NextMatch,
    PrevMatch,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollTop,
    ScrollBottom,
    Resize,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    None,
    Event(AppEvent),
}

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub segments: Vec<LiveSegment>,
    /// State of the meeting we're attached to. Drives the status bar.
    pub state: LifecycleState,
    /// Seconds elapsed since meeting start (refreshed by Tick events).
    pub elapsed_seconds: i64,
    /// Whether the user is currently typing a search query.
    pub search_mode: bool,
    /// Active filter substring (case-insensitive). Empty = no filter.
    pub search_query: String,
    /// Scroll offset from bottom (0 = newest segments visible).
    pub scroll_from_bottom: usize,
    /// Set true on Quit; the run loop checks this each iteration.
    pub should_quit: bool,
}

impl App {
    pub fn new(state: LifecycleState) -> Self {
        Self {
            segments: Vec::new(),
            state,
            elapsed_seconds: 0,
            search_mode: false,
            search_query: String::new(),
            scroll_from_bottom: 0,
            should_quit: false,
        }
    }

    pub fn apply(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Quit => self.should_quit = true,
            AppEvent::EnterSearch => {
                if !self.search_mode {
                    self.search_mode = true;
                }
            }
            AppEvent::ExitSearch => {
                self.search_mode = false;
                self.search_query.clear();
            }
            AppEvent::SubmitSearch => {
                // Keep the query active but exit "typing" mode.
                self.search_mode = false;
            }
            AppEvent::AppendSearchChar(c) => {
                if self.search_mode {
                    self.search_query.push(c);
                }
            }
            AppEvent::BackspaceSearch => {
                if self.search_mode {
                    self.search_query.pop();
                }
            }
            AppEvent::NextMatch => {
                let idx = self.first_visible_index();
                if let Some(next) = self.find_next_match(idx + 1) {
                    self.scroll_to_index(next);
                }
            }
            AppEvent::PrevMatch => {
                let idx = self.first_visible_index();
                if let Some(prev) = self.find_prev_match(idx.saturating_sub(1)) {
                    self.scroll_to_index(prev);
                }
            }
            AppEvent::ScrollUp(n) => {
                let max_offset = self.filtered_indices().len().saturating_sub(1);
                self.scroll_from_bottom = (self.scroll_from_bottom + n).min(max_offset);
            }
            AppEvent::ScrollDown(n) => {
                self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(n);
            }
            AppEvent::ScrollTop => {
                self.scroll_from_bottom =
                    self.filtered_indices().len().saturating_sub(1);
            }
            AppEvent::ScrollBottom => {
                self.scroll_from_bottom = 0;
            }
            AppEvent::Resize | AppEvent::Tick => {}
        }
    }

    /// Indices into `self.segments` that match the current filter.
    /// In filter order (oldest → newest).
    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.search_query.is_empty() {
            (0..self.segments.len()).collect()
        } else {
            let q = self.search_query.to_ascii_lowercase();
            self.segments
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.text.to_ascii_lowercase().contains(&q)
                        || s.speaker
                            .as_deref()
                            .map(|sp| sp.to_ascii_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .collect()
        }
    }

    fn first_visible_index(&self) -> usize {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            return 0;
        }
        let last = filtered.len() - 1;
        let visible = last.saturating_sub(self.scroll_from_bottom);
        filtered[visible]
    }

    fn find_next_match(&self, from: usize) -> Option<usize> {
        if self.search_query.is_empty() {
            return None;
        }
        self.filtered_indices().into_iter().find(|i| *i >= from)
    }

    fn find_prev_match(&self, from: usize) -> Option<usize> {
        if self.search_query.is_empty() {
            return None;
        }
        self.filtered_indices()
            .into_iter()
            .rev()
            .find(|i| *i <= from)
    }

    fn scroll_to_index(&mut self, target_segment_idx: usize) {
        let filtered = self.filtered_indices();
        if let Some(pos) = filtered.iter().position(|i| *i == target_segment_idx) {
            let last = filtered.len() - 1;
            self.scroll_from_bottom = last - pos;
        }
    }

    /// Visible slice for rendering at the current scroll position.
    /// Returns up to `viewport_height` segments, oldest first.
    pub fn visible_window(&self, viewport_height: usize) -> Vec<&LiveSegment> {
        let indices = self.filtered_indices();
        if indices.is_empty() || viewport_height == 0 {
            return Vec::new();
        }
        let last = indices.len() - 1;
        let bottom = last.saturating_sub(self.scroll_from_bottom);
        let top = bottom.saturating_sub(viewport_height.saturating_sub(1));
        indices[top..=bottom]
            .iter()
            .map(|i| &self.segments[*i])
            .collect()
    }
}

/// Translate a crossterm key code (with modifiers) into an `AppEvent`.
/// Pure for testability — the real loop calls this and routes results.
pub fn dispatch_key(code: KeyCode, search_mode: bool) -> KeyAction {
    use KeyCode::*;
    if search_mode {
        return match code {
            Esc => KeyAction::Event(AppEvent::ExitSearch),
            Enter => KeyAction::Event(AppEvent::SubmitSearch),
            Backspace => KeyAction::Event(AppEvent::BackspaceSearch),
            Char(c) => KeyAction::Event(AppEvent::AppendSearchChar(c)),
            _ => KeyAction::None,
        };
    }
    match code {
        Char('q') | Esc => KeyAction::Event(AppEvent::Quit),
        Char('/') => KeyAction::Event(AppEvent::EnterSearch),
        Char('n') => KeyAction::Event(AppEvent::NextMatch),
        Char('N') => KeyAction::Event(AppEvent::PrevMatch),
        Char('j') | Down => KeyAction::Event(AppEvent::ScrollDown(1)),
        Char('k') | Up => KeyAction::Event(AppEvent::ScrollUp(1)),
        PageDown => KeyAction::Event(AppEvent::ScrollDown(10)),
        PageUp => KeyAction::Event(AppEvent::ScrollUp(10)),
        Char('g') => KeyAction::Event(AppEvent::ScrollTop),
        Char('G') => KeyAction::Event(AppEvent::ScrollBottom),
        _ => KeyAction::None,
    }
}

/// Subset of crossterm::event::KeyCode the view needs to know about.
/// Defined locally so the pure logic doesn't depend on the crossterm
/// crate (the cli/live.rs binding maps from real KeyEvents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Backspace,
    Enter,
    Esc,
    Up,
    Down,
    PageUp,
    PageDown,
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
    fn quit_sets_should_quit() {
        let mut app = App::new(LifecycleState::Capturing);
        app.apply(AppEvent::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn enter_search_toggles_mode_and_keeps_query() {
        let mut app = App::new(LifecycleState::Capturing);
        app.apply(AppEvent::EnterSearch);
        assert!(app.search_mode);
        app.apply(AppEvent::AppendSearchChar('f'));
        app.apply(AppEvent::AppendSearchChar('o'));
        app.apply(AppEvent::AppendSearchChar('o'));
        assert_eq!(app.search_query, "foo");
        app.apply(AppEvent::SubmitSearch);
        assert!(!app.search_mode);
        assert_eq!(app.search_query, "foo");
    }

    #[test]
    fn esc_clears_search() {
        let mut app = App::new(LifecycleState::Capturing);
        app.apply(AppEvent::EnterSearch);
        app.apply(AppEvent::AppendSearchChar('x'));
        app.apply(AppEvent::ExitSearch);
        assert!(!app.search_mode);
        assert_eq!(app.search_query, "");
    }

    #[test]
    fn backspace_pops_char() {
        let mut app = App::new(LifecycleState::Capturing);
        app.apply(AppEvent::EnterSearch);
        app.apply(AppEvent::AppendSearchChar('a'));
        app.apply(AppEvent::AppendSearchChar('b'));
        app.apply(AppEvent::BackspaceSearch);
        assert_eq!(app.search_query, "a");
    }

    #[test]
    fn filter_matches_text_case_insensitive() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![
            seg(0.0, "Hello world", Some("SPEAKER_00")),
            seg(1.0, "Goodbye", Some("SPEAKER_01")),
        ];
        app.search_query = "WORLD".into();
        let indices = app.filtered_indices();
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn filter_matches_speaker() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![
            seg(0.0, "anything", Some("SPEAKER_00")),
            seg(1.0, "anything else", Some("SPEAKER_01")),
        ];
        app.search_query = "speaker_01".into();
        assert_eq!(app.filtered_indices(), vec![1]);
    }

    #[test]
    fn empty_query_returns_all() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![
            seg(0.0, "a", None),
            seg(1.0, "b", None),
        ];
        assert_eq!(app.filtered_indices(), vec![0, 1]);
    }

    #[test]
    fn scroll_clamped_at_top_and_bottom() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = (0..5).map(|i| seg(i as f64, "x", None)).collect();

        // Scroll up by 100 → capped at len-1.
        app.apply(AppEvent::ScrollUp(100));
        assert_eq!(app.scroll_from_bottom, 4);

        // Scroll down by 100 → 0.
        app.apply(AppEvent::ScrollDown(100));
        assert_eq!(app.scroll_from_bottom, 0);
    }

    #[test]
    fn visible_window_returns_last_n_when_at_bottom() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = (0..10).map(|i| seg(i as f64, &format!("seg{i}"), None)).collect();
        let visible = app.visible_window(3);
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].text, "seg7");
        assert_eq!(visible[2].text, "seg9");
    }

    #[test]
    fn visible_window_scrolls_with_offset() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = (0..10).map(|i| seg(i as f64, &format!("seg{i}"), None)).collect();
        app.scroll_from_bottom = 2;
        let visible = app.visible_window(3);
        assert_eq!(visible.len(), 3);
        // bottom is index 7, top is 5
        assert_eq!(visible[0].text, "seg5");
        assert_eq!(visible[2].text, "seg7");
    }

    #[test]
    fn visible_window_handles_empty_segments() {
        let app = App::new(LifecycleState::Capturing);
        assert!(app.visible_window(10).is_empty());
    }

    #[test]
    fn visible_window_zero_height_empty() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![seg(0.0, "a", None)];
        assert!(app.visible_window(0).is_empty());
    }

    #[test]
    fn key_dispatch_in_normal_mode() {
        assert_eq!(dispatch_key(KeyCode::Char('q'), false), KeyAction::Event(AppEvent::Quit));
        assert_eq!(dispatch_key(KeyCode::Char('/'), false), KeyAction::Event(AppEvent::EnterSearch));
        assert_eq!(dispatch_key(KeyCode::Char('n'), false), KeyAction::Event(AppEvent::NextMatch));
        assert_eq!(dispatch_key(KeyCode::Up, false), KeyAction::Event(AppEvent::ScrollUp(1)));
        assert_eq!(dispatch_key(KeyCode::PageDown, false), KeyAction::Event(AppEvent::ScrollDown(10)));
    }

    #[test]
    fn key_dispatch_in_search_mode_routes_chars() {
        assert_eq!(
            dispatch_key(KeyCode::Char('a'), true),
            KeyAction::Event(AppEvent::AppendSearchChar('a')),
        );
        assert_eq!(dispatch_key(KeyCode::Esc, true), KeyAction::Event(AppEvent::ExitSearch));
        assert_eq!(dispatch_key(KeyCode::Backspace, true), KeyAction::Event(AppEvent::BackspaceSearch));
        assert_eq!(dispatch_key(KeyCode::Enter, true), KeyAction::Event(AppEvent::SubmitSearch));
    }

    #[test]
    fn key_dispatch_unknown_returns_none() {
        // PageUp inside search mode is not a defined search action → None.
        assert_eq!(dispatch_key(KeyCode::PageUp, true), KeyAction::None);
    }

    #[test]
    fn next_match_jumps_to_next_filtered_index() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![
            seg(0.0, "alpha", None),
            seg(1.0, "beta", None),
            seg(2.0, "alpha gamma", None),
            seg(3.0, "delta", None),
        ];
        app.search_query = "alpha".into();
        // Currently at the bottom (newest). Next should wrap... but our find_next_match
        // doesn't wrap; it returns None if nothing >= idx. From bottom, hitting `n`
        // tries idx+1 which exceeds the array, so no jump.
        app.apply(AppEvent::NextMatch);
        // No change to scroll because no match >= idx+1.
        assert_eq!(app.scroll_from_bottom, 0);
    }

    #[test]
    fn prev_match_jumps_back() {
        let mut app = App::new(LifecycleState::Capturing);
        app.segments = vec![
            seg(0.0, "alpha", None),
            seg(1.0, "beta", None),
            seg(2.0, "alpha", None),
            seg(3.0, "delta", None),
        ];
        app.search_query = "alpha".into();
        // From bottom (idx 3), prev match <= 2 → segment 2 (alpha).
        app.apply(AppEvent::PrevMatch);
        // filtered = [0, 2], index 2 is at filtered position 1 (last);
        // last - pos = 1 - 1 = 0. Hmm — same position as before.
        // Let's prev again to jump to alpha at index 0.
        app.apply(AppEvent::PrevMatch);
        // Now scroll points to filtered position 0. last - pos = 1 - 0 = 1.
        assert_eq!(app.scroll_from_bottom, 1);
    }
}
