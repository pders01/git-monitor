use std::collections::HashSet;

use crate::diff::{DiffLine, FileDiff};
use crate::git::CommitEntry;

/// Which diff view is currently displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffView {
    Unstaged,
    Staged,
}

/// Input mode — determines how keystrokes are routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search, // typing in the /? search bar
}

/// Which screen is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Diff,      // current staged/unstaged diff view
    CommitLog, // list of recent commits
}

/// Tracks the current search query, matches, and navigation cursor.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub forward: bool,            // true = /, false = ?
    pub active: bool,             // matches exist and are navigable
    pub matches: Vec<(usize, usize, usize)>, // (line_idx, byte_start, byte_end)
    pub current_match: usize,
}

/// Central application state — owned exclusively by the main thread.
pub struct App {
    /// Whether the app should exit on the next loop iteration.
    pub should_quit: bool,
    /// Current diff view (staged vs unstaged).
    pub view: DiffView,
    /// Vertical scroll offset (in lines) into the diff output.
    pub scroll: u16,
    /// Total number of renderable diff lines (set after each git query).
    pub diff_line_count: u16,
    /// Height of the diff viewport in terminal rows (set each render).
    pub viewport_height: u16,

    /// Current screen being displayed.
    pub screen: Screen,
    /// Current input mode.
    pub input_mode: InputMode,
    /// Search state.
    pub search: SearchState,

    /// Recent commits from `git log`.
    pub commit_log: Vec<CommitEntry>,
    /// Cursor position in the commit log list.
    pub commit_log_selected: usize,

    /// When set, the main loop should suspend the TUI and pipe this
    /// content to the user's pager.
    pub pager_content: Option<String>,

    /// Filenames whose sections are currently collapsed.
    pub collapsed: HashSet<String>,
    /// Flattened diff lines including synthetic FileHeader entries.
    pub visible_lines: Vec<DiffLine>,
    /// Indices into `visible_lines` where FileHeader lines appear.
    pub file_header_positions: Vec<usize>,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            view: DiffView::Unstaged,
            scroll: 0,
            diff_line_count: 0,
            viewport_height: 0,
            screen: Screen::Diff,
            input_mode: InputMode::Normal,
            search: SearchState::default(),
            commit_log: Vec::new(),
            commit_log_selected: 0,
            pager_content: None,
            collapsed: HashSet::new(),
            visible_lines: Vec::new(),
            file_header_positions: Vec::new(),
        }
    }

    /// Toggle between staged and unstaged views, resetting scroll.
    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            DiffView::Unstaged => DiffView::Staged,
            DiffView::Staged => DiffView::Unstaged,
        };
        self.scroll = 0;
        self.clear_search();
    }

    /// Scroll down by `n` lines, clamped to content bounds.
    pub fn scroll_down(&mut self, n: u16) {
        let max = self.max_scroll();
        self.scroll = (self.scroll + n).min(max);
    }

    /// Scroll up by `n` lines, clamped to 0.
    pub fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Jump to the top of the diff.
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    /// Jump to the bottom of the diff.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.max_scroll();
    }

    /// Scroll down by half a page.
    pub fn scroll_half_down(&mut self) {
        let half = self.viewport_height / 2;
        self.scroll_down(half.max(1));
    }

    /// Scroll up by half a page.
    pub fn scroll_half_up(&mut self) {
        let half = self.viewport_height / 2;
        self.scroll_up(half.max(1));
    }

    fn max_scroll(&self) -> u16 {
        self.diff_line_count.saturating_sub(self.viewport_height)
    }

    // ── File sections (collapse / navigation) ──────────────────

    /// Rebuild `visible_lines` and `file_header_positions` from structured file diffs.
    pub fn recompute_visible_lines(&mut self, files: &[FileDiff]) {
        self.visible_lines.clear();
        self.file_header_positions.clear();

        for fd in files {
            // Skip empty-filename entries (e.g. from RepoState::empty)
            if !fd.filename.is_empty() {
                self.file_header_positions.push(self.visible_lines.len());
                self.visible_lines.push(DiffLine::FileHeader {
                    filename: fd.filename.clone(),
                    added: fd.added,
                    removed: fd.removed,
                });
            }

            if !self.collapsed.contains(&fd.filename) {
                self.visible_lines.extend(fd.lines.iter().cloned());
            }
        }

        self.diff_line_count = self.visible_lines.len() as u16;
        let max = self.max_scroll();
        if self.scroll > max {
            self.scroll = max;
        }
    }

    /// Jump scroll to the next file header after the current position.
    pub fn next_file(&mut self) {
        let current = self.scroll as usize;
        if let Some(&pos) = self
            .file_header_positions
            .iter()
            .find(|&&p| p > current)
        {
            self.scroll = (pos as u16).min(self.max_scroll());
        }
    }

    /// Jump scroll to the previous file header before the current position.
    pub fn prev_file(&mut self) {
        let current = self.scroll as usize;
        if let Some(&pos) = self
            .file_header_positions
            .iter()
            .rev()
            .find(|&&p| p < current)
        {
            self.scroll = pos as u16;
        }
    }

    /// Toggle the collapsed state of the file under the current viewport position.
    pub fn toggle_file_fold(&mut self, files: &[FileDiff]) {
        if let Some(name) = self.file_at_scroll() {
            if self.collapsed.contains(&name) {
                self.collapsed.remove(&name);
            } else {
                self.collapsed.insert(name);
            }
            self.recompute_visible_lines(files);
        }
    }

    /// Collapse all file sections.
    pub fn fold_all(&mut self, files: &[FileDiff]) {
        for fd in files {
            if !fd.filename.is_empty() {
                self.collapsed.insert(fd.filename.clone());
            }
        }
        self.recompute_visible_lines(files);
    }

    /// Expand all file sections.
    pub fn unfold_all(&mut self, files: &[FileDiff]) {
        self.collapsed.clear();
        self.recompute_visible_lines(files);
    }

    /// Determine which file the current scroll position is inside of.
    fn file_at_scroll(&self) -> Option<String> {
        let pos = self.scroll as usize;
        // Find the last file header at or before the scroll position
        let header_idx = self
            .file_header_positions
            .iter()
            .rposition(|&p| p <= pos)?;
        let line = &self.visible_lines[self.file_header_positions[header_idx]];
        if let DiffLine::FileHeader { filename, .. } = line {
            Some(filename.clone())
        } else {
            None
        }
    }

    // ── Commit log navigation ───────────────────────────────────

    pub fn commit_log_down(&mut self) {
        if !self.commit_log.is_empty() {
            self.commit_log_selected =
                (self.commit_log_selected + 1).min(self.commit_log.len() - 1);
        }
    }

    pub fn commit_log_up(&mut self) {
        self.commit_log_selected = self.commit_log_selected.saturating_sub(1);
    }

    // ── Search ──────────────────────────────────────────────────

    pub fn enter_search(&mut self, forward: bool) {
        self.input_mode = InputMode::Search;
        self.search = SearchState {
            query: String::new(),
            forward,
            active: false,
            matches: Vec::new(),
            current_match: 0,
        };
    }

    pub fn search_push(&mut self, c: char) {
        self.search.query.push(c);
    }

    pub fn search_pop(&mut self) {
        self.search.query.pop();
    }

    /// Confirm the search query and switch back to normal mode.
    pub fn search_confirm(&mut self, lines: &[DiffLine]) {
        self.input_mode = InputMode::Normal;
        self.recompute_matches(lines);
        if !self.search.matches.is_empty() {
            self.search.active = true;
            self.search.current_match = if self.search.forward {
                self.first_match_from(self.scroll as usize)
            } else {
                self.last_match_before(self.scroll as usize + self.viewport_height as usize)
            };
            self.jump_to_current_match();
        }
    }

    pub fn search_next(&mut self) {
        if !self.search.active || self.search.matches.is_empty() {
            return;
        }
        self.search.current_match =
            (self.search.current_match + 1) % self.search.matches.len();
        self.jump_to_current_match();
    }

    pub fn search_prev(&mut self) {
        if !self.search.active || self.search.matches.is_empty() {
            return;
        }
        let len = self.search.matches.len();
        self.search.current_match = if self.search.current_match == 0 {
            len - 1
        } else {
            self.search.current_match - 1
        };
        self.jump_to_current_match();
    }

    pub fn clear_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search = SearchState::default();
    }

    /// Recompute all search matches for the given lines (case-insensitive).
    pub fn recompute_matches(&mut self, lines: &[DiffLine]) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            self.search.active = false;
            return;
        }
        let query_lower = self.search.query.to_lowercase();
        for (line_idx, dl) in lines.iter().enumerate() {
            let text = dl.text();
            let lower = text.to_lowercase();
            let mut start = 0;
            while let Some(pos) = lower[start..].find(&query_lower) {
                let byte_start = start + pos;
                let byte_end = byte_start + query_lower.len();
                self.search.matches.push((line_idx, byte_start, byte_end));
                start = byte_end;
            }
        }
        self.search.active = !self.search.matches.is_empty();
    }

    fn jump_to_current_match(&mut self) {
        if let Some(&(line_idx, _, _)) = self.search.matches.get(self.search.current_match) {
            let target = (line_idx as u16).saturating_sub(5);
            self.scroll = target.min(self.max_scroll());
        }
    }

    fn first_match_from(&self, line: usize) -> usize {
        self.search
            .matches
            .iter()
            .position(|(li, _, _)| *li >= line)
            .unwrap_or(0)
    }

    fn last_match_before(&self, line: usize) -> usize {
        self.search
            .matches
            .iter()
            .rposition(|(li, _, _)| *li <= line)
            .unwrap_or(self.search.matches.len().saturating_sub(1))
    }
}
