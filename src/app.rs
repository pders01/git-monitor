/// Which diff view is currently displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffView {
    Unstaged,
    Staged,
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
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            view: DiffView::Unstaged,
            scroll: 0,
            diff_line_count: 0,
            viewport_height: 0,
        }
    }

    /// Toggle between staged and unstaged views, resetting scroll.
    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            DiffView::Unstaged => DiffView::Staged,
            DiffView::Staged => DiffView::Unstaged,
        };
        self.scroll = 0;
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

    fn max_scroll(&self) -> u16 {
        self.diff_line_count.saturating_sub(self.viewport_height)
    }
}
