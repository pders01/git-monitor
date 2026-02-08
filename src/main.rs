mod app;
mod diff;
mod event;
mod git;
mod pager;
mod ui;
mod watcher;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use clap::Parser;
use crossterm::{
    event::{self as ct_event, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, DiffView, InputMode, Screen};
use crate::diff::FileDiff;
use crate::event::AppEvent;
use crate::git::RepoState;

#[derive(Parser)]
#[command(name = "git-monitor", about = "Live Git diff TUI")]
struct Cli {
    /// Path to the git repository to watch (defaults to cwd)
    #[arg(default_value = ".")]
    repo: PathBuf,

    /// Debounce interval in milliseconds for filesystem events
    #[arg(long, default_value_t = 200)]
    debounce_ms: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo = cli.repo.canonicalize()?;
    if !repo.join(".git").exists() {
        bail!("{} is not a git repository", repo.display());
    }

    // ── Terminal setup ──────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Panic hook: restore terminal before printing the panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    // ── Run ─────────────────────────────────────────────────────
    let result = run(&mut terminal, &repo, &cli);

    // ── Terminal teardown ───────────────────────────────────────
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo: &Path,
    cli: &Cli,
) -> Result<()> {
    let mut app = App::new();
    let (tx, rx) = mpsc::channel::<AppEvent>();

    // Shared flag: when true, the keyboard thread stops reading events.
    // This prevents it from stealing keystrokes while an external pager runs.
    let kbd_paused = Arc::new(AtomicBool::new(false));

    // ── Keyboard + resize thread ────────────────────────────────
    let key_tx = tx.clone();
    let paused = Arc::clone(&kbd_paused);
    thread::spawn(move || loop {
        // When paused, spin-wait instead of reading from the terminal.
        if paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        // Poll with a timeout so we can check the pause flag periodically.
        match ct_event::poll(Duration::from_millis(100)) {
            Ok(true) => match ct_event::read() {
                Ok(Event::Key(key)) => {
                    if key_tx.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    if key_tx.send(AppEvent::Resize).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            Ok(false) => {} // timeout — loop back and check pause flag
            Err(_) => break,
        }
    });

    // ── Filesystem watcher thread ───────────────────────────────
    let _watcher = watcher::spawn(repo, cli.debounce_ms, tx)?;

    // ── Initial git query ───────────────────────────────────────
    let mut state = RepoState::query(repo).unwrap_or_else(|_| {
        RepoState::empty("Failed to query git state — is this a valid repo?")
    });
    app.recompute_visible_lines(current_files(&app, &state));

    // ── Main event loop ─────────────────────────────────────────
    terminal.draw(|frame| ui::draw(frame, &mut app, &state))?;

    while let Ok(event) = rx.recv() {
        match event {
            AppEvent::Key(key) => handle_key(&mut app, key, &state, repo),
            AppEvent::FsChange => {
                while let Ok(evt) = rx.try_recv() {
                    match evt {
                        AppEvent::FsChange => {}
                        AppEvent::Key(key) => handle_key(&mut app, key, &state, repo),
                        AppEvent::Resize => {}
                    }
                }
                state = RepoState::query(repo).unwrap_or(state);
                app.recompute_visible_lines(current_files(&app, &state));
                if app.search.active {
                    app.recompute_matches(&app.visible_lines.clone());
                }
            }
            AppEvent::Resize => {}
        }

        // ── Pager suspend/restore ───────────────────────────────
        if let Some(content) = app.pager_content.take() {
            // Stop the keyboard thread from reading the terminal
            kbd_paused.store(true, Ordering::Relaxed);
            // Give it time to finish any in-progress poll/read cycle
            thread::sleep(Duration::from_millis(150));

            // Leave TUI
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

            let pager_cmd = pager::detect_pager();
            let _ = pager::open_pager(&content, &pager_cmd);

            // Re-enter TUI
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            // Drain any events queued while the pager was active
            while rx.try_recv().is_ok() {}

            // Resume the keyboard thread
            kbd_paused.store(false, Ordering::Relaxed);
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|frame| ui::draw(frame, &mut app, &state))?;
    }

    Ok(())
}

/// Return the structured file diffs for the current view.
fn current_files<'a>(app: &App, state: &'a RepoState) -> &'a [FileDiff] {
    match app.view {
        DiffView::Unstaged => &state.unstaged_diff,
        DiffView::Staged => &state.staged_diff,
    }
}

/// Dispatch a single key event based on current input mode and screen.
fn handle_key(app: &mut App, key: KeyEvent, state: &RepoState, repo: &Path) {
    match app.input_mode {
        InputMode::Search => handle_search_input(app, key),
        InputMode::Normal => match app.screen {
            Screen::Diff => handle_diff_key(app, key, state, repo),
            Screen::CommitLog => handle_commit_log_key(app, key, repo),
        },
    }
}

// ── Search input mode ───────────────────────────────────────────

fn handle_search_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.clear_search(),
        KeyCode::Enter => {
            let lines = app.visible_lines.clone();
            app.search_confirm(&lines);
        }
        KeyCode::Backspace => app.search_pop(),
        KeyCode::Char(c) => app.search_push(c),
        _ => {}
    }
}

// ── Normal mode — Diff screen ───────────────────────────────────

fn handle_diff_key(app: &mut App, key: KeyEvent, state: &RepoState, repo: &Path) {
    match (key.code, key.modifiers) {
        // Quit
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        // View toggle
        (KeyCode::Tab, _) => {
            app.toggle_view();
            app.recompute_visible_lines(current_files(app, state));
        }
        // Basic scroll
        (KeyCode::Char('j') | KeyCode::Down, _) => app.scroll_down(1),
        (KeyCode::Char('k') | KeyCode::Up, _) => app.scroll_up(1),
        (KeyCode::Char('g'), _) => app.scroll_to_top(),
        (KeyCode::Char('G'), _) => app.scroll_to_bottom(),
        // Half-page scroll
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.scroll_half_down(),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.scroll_half_up(),
        // Full-page scroll
        (KeyCode::Char('f'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
            app.scroll_down(app.viewport_height)
        }
        (KeyCode::Char('b'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
            app.scroll_up(app.viewport_height)
        }
        // File navigation
        (KeyCode::Char(']'), _) => app.next_file(),
        (KeyCode::Char('['), _) => app.prev_file(),
        // File fold toggle
        (KeyCode::Char(' '), _) => {
            let files = current_files(app, state);
            app.toggle_file_fold(files);
        }
        // Collapse / expand all
        (KeyCode::Char('C'), _) => {
            let files = current_files(app, state);
            app.fold_all(files);
        }
        (KeyCode::Char('E'), _) => {
            let files = current_files(app, state);
            app.unfold_all(files);
        }
        // Search
        (KeyCode::Char('/'), _) => app.enter_search(true),
        (KeyCode::Char('?'), _) => app.enter_search(false),
        (KeyCode::Char('n'), _) => app.search_next(),
        (KeyCode::Char('N'), _) => app.search_prev(),
        (KeyCode::Esc, _) => app.clear_search(),
        // Pager — sends visible (expanded) lines
        (KeyCode::Char('d'), KeyModifiers::NONE) => {
            let content: String = app
                .visible_lines
                .iter()
                .map(|l| format!("{}\n", l.text()))
                .collect();
            if !content.trim().is_empty() {
                app.pager_content = Some(content);
            }
        }
        // Commit log
        (KeyCode::Char('l'), _) => {
            if let Ok(log) = git::git_log(repo, 50) {
                app.commit_log = log;
                app.commit_log_selected = 0;
                app.screen = Screen::CommitLog;
                app.clear_search();
            }
        }
        _ => {}
    }
}

// ── Normal mode — Commit Log screen ─────────────────────────────

fn handle_commit_log_key(app: &mut App, key: KeyEvent, repo: &Path) {
    match (key.code, key.modifiers) {
        // Back to diff
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
            app.screen = Screen::Diff;
            app.clear_search();
        }
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        // Navigate
        (KeyCode::Char('j') | KeyCode::Down, _) => app.commit_log_down(),
        (KeyCode::Char('k') | KeyCode::Up, _) => app.commit_log_up(),
        (KeyCode::Char('g'), _) => app.commit_log_selected = 0,
        (KeyCode::Char('G'), _) => {
            if !app.commit_log.is_empty() {
                app.commit_log_selected = app.commit_log.len() - 1;
            }
        }
        // View commit in external pager
        (KeyCode::Enter, _) | (KeyCode::Char('d'), KeyModifiers::NONE) => {
            if let Some(entry) = app.commit_log.get(app.commit_log_selected) {
                if let Ok(raw) = git::git_show(repo, &entry.hash) {
                    app.pager_content = Some(raw);
                }
            }
        }
        // Search commit messages
        (KeyCode::Char('/'), _) => app.enter_search(true),
        (KeyCode::Char('?'), _) => app.enter_search(false),
        (KeyCode::Char('n'), _) => app.search_next(),
        (KeyCode::Char('N'), _) => app.search_prev(),
        _ => {}
    }
}
