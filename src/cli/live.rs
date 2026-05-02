use crate::config::Config;
use crate::lifecycle::lockfile::{LifecycleState, read_active_meeting};
use crate::live::tui::{App, AppEvent, KeyAction, KeyCode, dispatch_key};
use crate::paths::Paths;
use crate::transcribe::jsonl::read_segments;
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::Args;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode as CtKeyCode, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::Stdout;
use std::path::Path;
use std::time::Duration;
use tokio::time::interval;

#[derive(Args, Debug)]
pub struct LiveArgs {}

pub async fn run(_args: LiveArgs, _config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let am = read_active_meeting(&paths.active_meeting_file())?
        .ok_or_else(|| anyhow!("no active meeting — run `furu start` first"))?;

    let live_jsonl = am.runtime_dir.join("transcript.live.jsonl");
    if !live_jsonl.exists() {
        // Create empty so the first poll doesn't error.
        std::fs::File::create(&live_jsonl)
            .with_context(|| format!("could not create {}", live_jsonl.display()))?;
    }

    let mut app = App::new(am.state);
    app.segments = read_segments(&live_jsonl).unwrap_or_default();
    let started_at = am.started_at;
    app.elapsed_seconds = (Utc::now() - started_at).num_seconds();

    let mut terminal = init_terminal()?;
    let result = event_loop(&mut terminal, &mut app, &live_jsonl, &paths, started_at).await;
    restore_terminal(&mut terminal)?;
    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    live_jsonl: &Path,
    paths: &Paths,
    started_at: chrono::DateTime<Utc>,
) -> Result<()> {
    let mut tick = interval(Duration::from_millis(250));
    let mut event_stream = crossterm::event::EventStream::new();
    use futures::StreamExt;

    loop {
        terminal.draw(|frame| render(frame, app))?;

        tokio::select! {
            _ = tick.tick() => {
                // Re-read the JSONL (cheap — small files).
                if let Ok(segs) = read_segments(live_jsonl) {
                    let was_at_bottom = app.scroll_from_bottom == 0;
                    let new_count = segs.len();
                    let old_count = app.segments.len();
                    app.segments = segs;
                    if !was_at_bottom && new_count > old_count {
                        // Preserve scroll position relative to the previous bottom.
                        app.scroll_from_bottom += new_count - old_count;
                    }
                }
                if let Ok(Some(am)) = read_active_meeting(&paths.active_meeting_file()) {
                    app.state = am.state;
                }
                app.elapsed_seconds = (Utc::now() - started_at).num_seconds();
                if app.state != LifecycleState::Capturing {
                    // Render one final frame at the new state, then keep
                    // looping so the user can scroll/search the result.
                }
            }
            ev = event_stream.next() => {
                match ev {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        let code = map_key(key.code);
                        if let Some(c) = code {
                            if let KeyAction::Event(e) = dispatch_key(c, app.search_mode) {
                                app.apply(e);
                            }
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        app.apply(AppEvent::Resize);
                    }
                    Some(Ok(_)) | None => {}
                    Some(Err(e)) => {
                        return Err(anyhow!("terminal event error: {e}"));
                    }
                }
            }
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

fn map_key(code: CtKeyCode) -> Option<KeyCode> {
    match code {
        CtKeyCode::Char(c) => Some(KeyCode::Char(c)),
        CtKeyCode::Backspace => Some(KeyCode::Backspace),
        CtKeyCode::Enter => Some(KeyCode::Enter),
        CtKeyCode::Esc => Some(KeyCode::Esc),
        CtKeyCode::Up => Some(KeyCode::Up),
        CtKeyCode::Down => Some(KeyCode::Down),
        CtKeyCode::PageUp => Some(KeyCode::PageUp),
        CtKeyCode::PageDown => Some(KeyCode::PageDown),
        _ => None,
    }
}

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let viewport = layout[0];
    let status = layout[1];

    // Body
    let lines: Vec<Line<'_>> = app
        .visible_window(viewport.height as usize)
        .into_iter()
        .map(|s| {
            let speaker = s.speaker.as_deref().unwrap_or("(speaker unknown)");
            Line::from(vec![
                Span::styled(format!("[{}] ", s.t), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{speaker}: "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(s.text.trim().to_string()),
            ])
        })
        .collect();

    let title = if app.search_query.is_empty() {
        "transcript".to_string()
    } else if app.search_mode {
        format!("transcript — search: {}_", app.search_query)
    } else {
        format!("transcript — filter: {}", app.search_query)
    };
    let body = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(body, viewport);

    // Status bar
    let elapsed = format_elapsed(app.elapsed_seconds);
    let state_label = match app.state {
        LifecycleState::Capturing => Span::styled(
            " [REC] ",
            Style::default().bg(Color::Red).fg(Color::White),
        ),
        LifecycleState::Finalizing => Span::styled(
            " [FINALIZING] ",
            Style::default().bg(Color::Yellow).fg(Color::Black),
        ),
        LifecycleState::Done => Span::styled(
            " [DONE] ",
            Style::default().bg(Color::Green).fg(Color::Black),
        ),
    };
    let hint = if app.search_mode {
        "  ESC clear, Enter accept"
    } else {
        "  q quit, / search, n/N next/prev, ↑↓ scroll, g/G top/bottom"
    };
    let status_line = Line::from(vec![
        state_label,
        Span::raw(format!(" {elapsed}  ")),
        Span::raw(format!("({} segments) ", app.segments.len())),
        Span::styled(hint, Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(status_line), status);
}

fn format_elapsed(seconds: i64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("could not enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("could not switch to alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("could not init terminal")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).ok();
    terminal.show_cursor().ok();
    Ok(())
}
