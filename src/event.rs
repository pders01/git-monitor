use crossterm::event::KeyEvent;

/// All events funnelled through the main loop's mpsc channel.
pub enum AppEvent {
    /// A keypress from the keyboard-reading thread.
    Key(KeyEvent),
    /// The filesystem watcher detected a change (debounced).
    FsChange,
    /// The terminal was resized — triggers a re-render.
    Resize,
}
