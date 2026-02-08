use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, DiffView};
use crate::diff::DiffLine;
use crate::git::RepoState;

/// Render the full TUI frame.
pub fn draw(frame: &mut Frame, app: &mut App, state: &RepoState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(1),   // diff area
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    // ── Status bar ──────────────────────────────────────────────
    let branch = &state.branch;
    let short_sha = state
        .last_commit_hash
        .as_deref()
        .filter(|h| h.len() >= 7)
        .map(|h| &h[..7])
        .unwrap_or("-------");
    let commit_msg = state
        .last_commit_message
        .as_deref()
        .unwrap_or("(no commits)");
    let elapsed = state.refreshed_at.elapsed().as_secs();
    let ago = if elapsed == 0 {
        String::from("just now")
    } else if elapsed < 60 {
        format!("{elapsed}s ago")
    } else {
        format!("{}m ago", elapsed / 60)
    };
    let status_text = format!(
        " {branch} | {short_sha} {commit_msg} | {} staged, {} unstaged  {ago}",
        state.staged_count, state.unstaged_count,
    );
    let status_bar = Paragraph::new(Line::from(vec![Span::styled(
        status_text,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]))
    .style(Style::default().bg(Color::Cyan));
    frame.render_widget(status_bar, chunks[0]);

    // ── Diff area ───────────────────────────────────────────────
    let view_label = match app.view {
        DiffView::Unstaged => " Unstaged Changes ",
        DiffView::Staged => " Staged Changes ",
    };
    let diff_lines = match app.view {
        DiffView::Unstaged => &state.unstaged_diff,
        DiffView::Staged => &state.staged_diff,
    };

    app.diff_line_count = diff_lines.len() as u16;
    // viewport_height = diff area height minus 2 for the block borders
    app.viewport_height = chunks[1].height.saturating_sub(2);

    // Clamp scroll if content shrank (e.g. after a commit or file revert)
    let max_scroll = app.diff_line_count.saturating_sub(app.viewport_height);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    let styled_lines: Vec<Line> = diff_lines.iter().map(style_diff_line).collect();

    let diff_widget = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(view_label)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .scroll((app.scroll, 0));
    frame.render_widget(diff_widget, chunks[1]);

    // ── Help bar ────────────────────────────────────────────────
    let help = " q: quit | Tab: staged/unstaged | j/k: scroll | g/G: top/bottom | PgUp/PgDn ";
    let help_bar = Paragraph::new(Line::from(Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(help_bar, chunks[2]);
}

/// Map a parsed `DiffLine` to a coloured ratatui `Line`.
fn style_diff_line(dl: &DiffLine) -> Line<'_> {
    match dl {
        DiffLine::Header(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        DiffLine::Hunk(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default().fg(Color::Cyan),
        )),
        DiffLine::Added(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default().fg(Color::Green),
        )),
        DiffLine::Removed(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default().fg(Color::Red),
        )),
        DiffLine::Context(s) => Line::from(Span::raw(s.as_str())),
    }
}
