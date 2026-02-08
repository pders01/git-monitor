mod app;
mod diff;
mod event;
mod git;
mod ui;
mod watcher;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use anyhow::{bail, Result};
use clap::Parser;
use crossterm::{
    event::{self as ct_event, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::App;
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
    repo: &PathBuf,
    cli: &Cli,
) -> Result<()> {
    let mut app = App::new();
    let (tx, rx) = mpsc::channel::<AppEvent>();

    // ── Keyboard + resize thread ────────────────────────────────
    let key_tx = tx.clone();
    thread::spawn(move || loop {
        match ct_event::read() {
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
            Ok(_) => {}    // ignore mouse events, etc.
            Err(_) => break, // terminal I/O error — stop the thread
        }
    });

    // ── Filesystem watcher thread ───────────────────────────────
    // The debouncer manages its own internal thread; we keep the handle
    // alive so the watcher isn't dropped.
    let _watcher = watcher::spawn(repo, cli.debounce_ms, tx)?;

    // ── Initial git query ───────────────────────────────────────
    let mut state = RepoState::query(repo).unwrap_or_else(|_| {
        RepoState::empty("Failed to query git state — is this a valid repo?")
    });

    // ── Main event loop ─────────────────────────────────────────
    terminal.draw(|frame| ui::draw(frame, &mut app, &state))?;

    loop {
        // Block until the next event arrives
        let event = match rx.recv() {
            Ok(e) => e,
            Err(_) => break, // all senders dropped
        };

        match event {
            AppEvent::Key(key) => handle_key(&mut app, key),
            AppEvent::FsChange => {
                // Drain queued events — process non-FsChange events inline
                while let Ok(evt) = rx.try_recv() {
                    match evt {
                        AppEvent::FsChange => {} // coalesce
                        AppEvent::Key(key) => handle_key(&mut app, key),
                        AppEvent::Resize => {}   // will re-render below
                    }
                }

                state = RepoState::query(repo).unwrap_or(state);
            }
            AppEvent::Resize => {
                // Just re-render — ratatui handles the new size automatically
            }
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|frame| ui::draw(frame, &mut app, &state))?;
    }

    Ok(())
}

/// Dispatch a single key event to update app state.
fn handle_key(app: &mut App, key: KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        (KeyCode::Tab, _) => app.toggle_view(),
        (KeyCode::Char('j') | KeyCode::Down, _) => app.scroll_down(1),
        (KeyCode::Char('k') | KeyCode::Up, _) => app.scroll_up(1),
        (KeyCode::Char('g'), _) => app.scroll_to_top(),
        (KeyCode::Char('G'), _) => app.scroll_to_bottom(),
        (KeyCode::PageDown, _) => app.scroll_down(app.viewport_height),
        (KeyCode::PageUp, _) => app.scroll_up(app.viewport_height),
        _ => {}
    }
}
