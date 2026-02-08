use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, DiffView, InputMode, Screen, SearchState};
use crate::diff::DiffLine;
use crate::git::RepoState;

/// Render the full TUI frame.
pub fn draw(frame: &mut Frame, app: &mut App, state: &RepoState) {
    match app.screen {
        Screen::Diff => draw_diff_screen(frame, app, state),
        Screen::CommitLog => draw_commit_log_screen(frame, app, state),
    }
}

// ── Diff screen (staged/unstaged) ───────────────────────────────

fn draw_diff_screen(frame: &mut Frame, app: &mut App, state: &RepoState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(1),   // diff area
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    draw_status_bar(frame, state, chunks[0]);

    let view_label = match app.view {
        DiffView::Unstaged => " Unstaged Changes ",
        DiffView::Staged => " Staged Changes ",
    };

    app.diff_line_count = app.visible_lines.len() as u16;
    app.viewport_height = chunks[1].height.saturating_sub(2);

    let max_scroll = app.diff_line_count.saturating_sub(app.viewport_height);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    let term_width = chunks[1].width.saturating_sub(2) as usize; // minus block borders
    let styled_lines: Vec<Line> = app
        .visible_lines
        .iter()
        .enumerate()
        .map(|(i, dl)| highlight_diff_line(dl, i, &app.search, &app.collapsed, term_width))
        .collect();

    let diff_widget = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(view_label)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .scroll((app.scroll, 0));
    frame.render_widget(diff_widget, chunks[1]);

    draw_help_bar(frame, app, chunks[2]);
}

// ── Commit Log screen ───────────────────────────────────────────

fn draw_commit_log_screen(frame: &mut Frame, app: &mut App, state: &RepoState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(1),   // commit list
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    draw_status_bar(frame, state, chunks[0]);

    app.viewport_height = chunks[1].height.saturating_sub(2);

    // Responsive column layout based on available terminal width.
    // Layout: [prefix 2][hash 8][sp 1][message ...][sp 2][author N][sp 2][date N]
    // The message column gets whatever space remains after fixed columns.
    let available = chunks[1].width.saturating_sub(2) as usize; // minus block borders
    let date_width = app
        .commit_log
        .iter()
        .map(|e| e.date_relative.len())
        .max()
        .unwrap_or(8)
        .min(16);
    let author_width = app
        .commit_log
        .iter()
        .map(|e| e.author.len())
        .max()
        .unwrap_or(8)
        .min(20);
    // Fixed: prefix(2) + hash(8) + gaps(1+2+2) = 15
    let fixed = 15 + author_width + date_width;
    let msg_width = available.saturating_sub(fixed).max(10);

    let mut lines: Vec<Line> = Vec::new();
    for (i, entry) in app.commit_log.iter().enumerate() {
        let is_selected = i == app.commit_log_selected;

        let prefix = if is_selected { "> " } else { "  " };
        let msg_display = truncate_str(&entry.message, msg_width);
        let author_display = truncate_str(&entry.author, author_width);

        if is_selected {
            let text = format!(
                "{prefix}{:<8} {:<msg_w$}  {:<auth_w$}  {}",
                entry.hash,
                msg_display,
                author_display,
                entry.date_relative,
                msg_w = msg_width,
                auth_w = author_width,
            );
            lines.push(Line::from(Span::styled(
                text,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            let is_search_match = app.search.active
                && !app.search.query.is_empty()
                && {
                    let q = app.search.query.to_lowercase();
                    entry.message.to_lowercase().contains(&q)
                        || entry.author.to_lowercase().contains(&q)
                        || entry.hash.to_lowercase().contains(&q)
                };
            let underline = if is_search_match {
                Modifier::UNDERLINED
            } else {
                Modifier::empty()
            };

            lines.push(Line::from(vec![
                Span::styled(
                    prefix.to_string(),
                    Style::default().fg(Color::DarkGray).add_modifier(underline),
                ),
                Span::styled(
                    format!("{:<8}", entry.hash),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | underline),
                ),
                Span::styled(
                    " ".to_string(),
                    Style::default().add_modifier(underline),
                ),
                Span::styled(
                    format!("{:<msg_w$}", msg_display, msg_w = msg_width),
                    Style::default().fg(Color::White).add_modifier(underline),
                ),
                Span::styled(
                    "  ".to_string(),
                    Style::default().add_modifier(underline),
                ),
                Span::styled(
                    format!("{:<auth_w$}", author_display, auth_w = author_width),
                    Style::default().fg(Color::Cyan).add_modifier(underline),
                ),
                Span::styled(
                    "  ".to_string(),
                    Style::default().add_modifier(underline),
                ),
                Span::styled(
                    entry.date_relative.clone(),
                    Style::default().fg(Color::DarkGray).add_modifier(underline),
                ),
            ]));
        }
    }

    // Keep selected item in view
    let list_scroll = if app.commit_log_selected as u16 >= app.viewport_height {
        (app.commit_log_selected as u16) - app.viewport_height + 1
    } else {
        0
    };

    let list_widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Commit Log ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .scroll((list_scroll, 0));
    frame.render_widget(list_widget, chunks[1]);

    draw_help_bar(frame, app, chunks[2]);
}

// ── Shared widgets ──────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, state: &RepoState, area: ratatui::layout::Rect) {
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
    frame.render_widget(status_bar, area);
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help_text = match app.input_mode {
        InputMode::Search => {
            let prefix = if app.search.forward { "/" } else { "?" };
            format!("{prefix}{}█", app.search.query)
        }
        InputMode::Normal => {
            if app.search.active && !app.search.matches.is_empty() {
                let total = app.search.matches.len();
                let current = app.search.current_match + 1;
                format!(
                    " [{current}/{total}] \"{}\"  n/N: next/prev | Esc: clear",
                    app.search.query
                )
            } else {
                match app.screen {
                    Screen::Diff => {
                        " q: quit | Tab: staged/unstaged | j/k: scroll | ]/[: file | Space: fold | C/E: all | /: search | d: pager | l: log ".to_string()
                    }
                    Screen::CommitLog => {
                        " q/Esc: back | j/k: navigate | Enter/d: view in pager | /: search ".to_string()
                    }
                }
            }
        }
    };

    let style = if app.input_mode == InputMode::Search {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else if app.search.active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let help_bar = Paragraph::new(Line::from(Span::styled(help_text, style)));
    frame.render_widget(help_bar, area);
}

// ── Diff line styling with search highlight ─────────────────────

/// Map a `DiffLine` to a coloured `Line`, with search matches highlighted.
fn highlight_diff_line(
    dl: &DiffLine,
    line_idx: usize,
    search: &SearchState,
    collapsed: &std::collections::HashSet<String>,
    term_width: usize,
) -> Line<'static> {
    // Special rendering for file section headers
    if let DiffLine::FileHeader {
        filename,
        added,
        removed,
    } = dl
    {
        return render_file_header(filename, *added, *removed, collapsed, term_width);
    }

    let base_style = match dl {
        DiffLine::FileHeader { .. } => unreachable!(),
        DiffLine::Header(_) => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        DiffLine::Hunk(_) => Style::default().fg(Color::Cyan),
        DiffLine::Added(_) => Style::default().fg(Color::Green),
        DiffLine::Removed(_) => Style::default().fg(Color::Red),
        DiffLine::Context(_) => Style::default(),
    };

    let text = dl.text().to_string();

    if !search.active || search.query.is_empty() || search.matches.is_empty() {
        return Line::from(Span::styled(text, base_style));
    }

    // Collect matches for this line
    let line_matches: Vec<(usize, usize, bool)> = search
        .matches
        .iter()
        .enumerate()
        .filter(|(_, (li, _, _))| *li == line_idx)
        .map(|(match_idx, (_, start, end))| {
            let is_current = match_idx == search.current_match;
            (*start, *end, is_current)
        })
        .collect();

    if line_matches.is_empty() {
        return Line::from(Span::styled(text, base_style));
    }

    let mut spans = Vec::new();
    let mut pos = 0;
    for (start, end, is_current) in &line_matches {
        let start = (*start).min(text.len());
        let end = (*end).min(text.len());
        if pos < start {
            spans.push(Span::styled(text[pos..start].to_string(), base_style));
        }
        let highlight_style = if *is_current {
            Style::default().bg(Color::Red).fg(Color::White)
        } else {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };
        spans.push(Span::styled(text[start..end].to_string(), highlight_style));
        pos = end;
    }
    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), base_style));
    }

    Line::from(spans)
}

/// Render a file section header: `▾/▸ filename   +N -M` with full-width bar.
fn render_file_header(
    filename: &str,
    added: usize,
    removed: usize,
    collapsed: &std::collections::HashSet<String>,
    term_width: usize,
) -> Line<'static> {
    let is_collapsed = collapsed.contains(filename);
    let arrow = if is_collapsed { "▸ " } else { "▾ " };
    let stats = format!("+{added} -{removed}");

    let bg = Style::default()
        .bg(Color::DarkGray)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    // Calculate padding between filename and stats
    let content_len = arrow.len() + filename.len() + stats.len() + 2; // +2 for spaces around stats
    let padding = if term_width > content_len {
        term_width - content_len
    } else {
        1
    };

    Line::from(vec![
        Span::styled(arrow.to_string(), bg),
        Span::styled(filename.to_string(), bg),
        Span::styled(" ".repeat(padding), bg),
        Span::styled(
            format!("+{added}"),
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ".to_string(),
            bg,
        ),
        Span::styled(
            format!("-{removed}"),
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

// ── Helpers ─────────────────────────────────────────────────────

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s[..max_len].to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
