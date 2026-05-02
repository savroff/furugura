//! U7 integration tests for the live TUI app state.
//!
//! Real terminal driving (crossterm + ratatui) is exercised by hand
//! during development. This file tests the public state surface end-to-end.

use furugura::lifecycle::lockfile::LifecycleState;
use furugura::live::tui::{App, KeyAction, KeyCode, dispatch_key};
use furugura::transcribe::LiveSegment;

fn seg(start: f64, text: &str) -> LiveSegment {
    LiveSegment {
        t: format!("00:00:{:02}.000", start as i64),
        speaker: None,
        text: text.into(),
        start,
        end: start + 1.0,
    }
}

#[test]
fn search_filters_to_matching_segments() {
    let mut app = App::new(LifecycleState::Capturing);
    app.segments = vec![
        seg(0.0, "discuss roadmap"),
        seg(1.0, "review PR"),
        seg(2.0, "roadmap is approved"),
        seg(3.0, "schedule follow-up"),
    ];
    app.search_query = "roadmap".into();
    let visible = app.visible_window(10);
    assert_eq!(visible.len(), 2);
    assert!(visible[0].text.contains("roadmap"));
    assert!(visible[1].text.contains("roadmap"));
}

#[test]
fn slash_enters_search_then_typing_appends() {
    let mut app = App::new(LifecycleState::Capturing);
    if let KeyAction::Event(e) = dispatch_key(KeyCode::Char('/'), false) {
        app.apply(e);
    }
    assert!(app.search_mode);
    for c in "PR".chars() {
        if let KeyAction::Event(e) = dispatch_key(KeyCode::Char(c), true) {
            app.apply(e);
        }
    }
    assert_eq!(app.search_query, "PR");
}

#[test]
fn esc_in_search_clears_query() {
    let mut app = App::new(LifecycleState::Capturing);
    app.search_mode = true;
    app.search_query = "old".into();
    if let KeyAction::Event(e) = dispatch_key(KeyCode::Esc, true) {
        app.apply(e);
    }
    assert!(!app.search_mode);
    assert_eq!(app.search_query, "");
}

#[test]
fn finalizing_state_visible_in_app() {
    let mut app = App::new(LifecycleState::Capturing);
    app.state = LifecycleState::Finalizing;
    // The app keeps rendering — the state-bar update is rendered, but the
    // pure App doesn't error. Lines below smoke-test that the App accepts
    // segment updates after the state changes.
    app.segments.push(seg(0.0, "post-stop"));
    let v = app.visible_window(5);
    assert_eq!(v.len(), 1);
}
