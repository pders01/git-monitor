# git-monitor

A terminal user interface (TUI) that watches your Git repository and streams diffs in real time. See exactly what changed the instant you save a file.

![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- **Live Diff Streaming** - Filesystem watcher with debounced refresh shows changes the instant you save
- **Collapsible File Sections** - Per-file headers with `+/-` stats, fold/unfold individual files or all at once
- **Staged / Unstaged Toggle** - Switch between working-tree and index diffs with `Tab`
- **Search** - `/` and `?` with `n`/`N` navigation and highlighted matches
- **Commit Log** - Browse recent commits with columnar layout (hash, message, author, date)
- **External Pager** - View diffs or commits in your configured pager (less, delta, bat, etc.)
- **Vim Keybindings** - Navigate with familiar vim motions (`j/k`, `Ctrl-d/u`, `g/G`)
- **Gitignore Aware** - Filesystem watcher respects `.gitignore` rules

## Installation

### Pre-built binaries

#### macOS (Apple Silicon)

```bash
curl -fsSL https://github.com/pders01/git-monitor/releases/latest/download/git-monitor-macos-aarch64.tar.gz \
  | tar xz && sudo mv git-monitor /usr/local/bin/
```

#### macOS (Intel)

```bash
curl -fsSL https://github.com/pders01/git-monitor/releases/latest/download/git-monitor-macos-x86_64.tar.gz \
  | tar xz && sudo mv git-monitor /usr/local/bin/
```

#### Linux (x86_64)

```bash
curl -fsSL https://github.com/pders01/git-monitor/releases/latest/download/git-monitor-linux-x86_64.tar.gz \
  | tar xz && sudo mv git-monitor /usr/local/bin/
```

#### Linux (aarch64)

```bash
curl -fsSL https://github.com/pders01/git-monitor/releases/latest/download/git-monitor-linux-aarch64.tar.gz \
  | tar xz && sudo mv git-monitor /usr/local/bin/
```

#### Windows

Download `git-monitor-windows-x86_64.zip` from the [Releases](https://github.com/pders01/git-monitor/releases/latest) page and add `git-monitor.exe` to your PATH.

### From source

```bash
git clone https://github.com/pders01/git-monitor.git
cd git-monitor
cargo install --path .
```

## Usage

```bash
git-monitor              # watch current directory
git-monitor /path/to/repo  # watch a specific repo
git-monitor --debounce-ms 500  # custom debounce interval (default: 200ms)
```

### Keybindings

#### Navigation

| Key | Action |
|-----|--------|
| `q` / `Ctrl-c` | Quit |
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `g` | Go to top |
| `G` | Go to bottom |
| `Ctrl-d` | Half-page down |
| `Ctrl-u` | Half-page up |
| `Ctrl-f` / `PageDown` | Full page down |
| `Ctrl-b` / `PageUp` | Full page up |

#### Diff View

| Key | Action |
|-----|--------|
| `Tab` | Toggle staged / unstaged diff |
| `]` | Jump to next file |
| `[` | Jump to previous file |
| `Space` | Toggle fold for current file |
| `C` | Collapse all files |
| `E` | Expand all files |
| `d` | View in external pager |
| `l` | Open commit log |

#### Search

| Key | Action |
|-----|--------|
| `/` | Search forward |
| `?` | Search backward |
| `n` | Next match |
| `N` | Previous match |
| `Esc` | Clear search |

#### Commit Log

| Key | Action |
|-----|--------|
| `q` / `Esc` | Back to diff view |
| `j` / `k` | Navigate commits |
| `Enter` / `d` | View commit in pager |
| `/` | Search commits |

### External Pager

git-monitor detects your preferred pager in this order:

1. `GIT_PAGER` environment variable
2. `git config core.pager`
3. `PAGER` environment variable
4. `less` (fallback)

This works with diff-aware pagers like [delta](https://github.com/dandavison/delta) and [bat](https://github.com/sharkdp/bat).

## Architecture

```
src/
├── main.rs     # Entry point, CLI parsing, event loop, TUI suspend/resume
├── app.rs      # Application state, scroll, search, collapse, file navigation
├── diff.rs     # Diff parser — raw git output → FileDiff sections → DiffLine types
├── git.rs      # Git CLI wrapper — branch, status, diff, log, show
├── event.rs    # Event types (Key, FsChange, Resize)
├── ui.rs       # Rendering — diff view, commit log, status bar, help bar
├── pager.rs    # External pager detection and invocation
└── watcher.rs  # Filesystem watcher with gitignore filtering
```

### Three-thread design

```
Keyboard thread ──→ mpsc channel ──→ Main thread (event loop + render)
FS watcher thread ─┘
```

No async runtime — just `std::sync::mpsc` and `std::thread`. The keyboard thread uses `poll(100ms)` with a pause flag so it can yield the terminal to external pagers.

## Development

This project uses [just](https://github.com/casey/just) as a command runner:

```bash
just          # list available recipes
just build    # cargo build
just test     # cargo test
just clippy   # cargo clippy -- -D warnings
just fmt      # cargo fmt
just ci       # run fmt-check, clippy, and tests
```

## Dependencies

- [ratatui](https://github.com/ratatui-org/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal UI
- [notify](https://github.com/notify-rs/notify) + [notify-debouncer-mini](https://docs.rs/notify-debouncer-mini) - Filesystem watching
- [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) - Gitignore filtering (same crate as ripgrep)
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [anyhow](https://github.com/dtolnay/anyhow) - Error handling

## License

MIT License - see [LICENSE](LICENSE) for details.
