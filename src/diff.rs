/// A single line from a unified diff, classified by type.
#[derive(Debug, Clone)]
pub enum DiffLine {
    /// Synthetic header for a per-file collapsible section.
    FileHeader {
        filename: String,
        added: usize,
        removed: usize,
    },
    /// `diff --git …`, `index …`, `--- a/…`, `+++ b/…`
    Header(String),
    /// `@@ -n,m +n,m @@` hunk header
    Hunk(String),
    /// `+…` added line
    Added(String),
    /// `-…` removed line
    Removed(String),
    /// ` …` context (unchanged) line
    Context(String),
}

impl DiffLine {
    /// Return the inner text content regardless of variant.
    pub fn text(&self) -> &str {
        match self {
            DiffLine::FileHeader { filename, .. } => filename,
            DiffLine::Header(s)
            | DiffLine::Hunk(s)
            | DiffLine::Added(s)
            | DiffLine::Removed(s)
            | DiffLine::Context(s) => s,
        }
    }
}

/// All diff content for a single file.
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub filename: String,
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<DiffLine>,
}

/// Parse raw `git diff` output into per-file sections.
///
/// Splits on `diff --git` boundaries, extracts the filename from the `b/` path,
/// counts added/removed lines, and collects per-file diff lines.
pub fn parse_files(raw: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in raw.lines() {
        if line.starts_with("diff --git ") && !current_lines.is_empty() {
            files.push(build_file_diff(&current_lines));
            current_lines.clear();
        }
        current_lines.push(line);
    }
    if !current_lines.is_empty() {
        files.push(build_file_diff(&current_lines));
    }
    files
}

/// Build a `FileDiff` from a slice of raw lines belonging to one file.
fn build_file_diff(raw_lines: &[&str]) -> FileDiff {
    let filename = extract_filename(raw_lines[0]);
    let mut added = 0;
    let mut removed = 0;
    let mut lines = Vec::new();

    for &line in raw_lines {
        let dl = if line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("Binary files ")
        {
            DiffLine::Header(line.to_string())
        } else if line.starts_with("@@") {
            DiffLine::Hunk(line.to_string())
        } else if line.starts_with('+') {
            added += 1;
            DiffLine::Added(line.to_string())
        } else if line.starts_with('-') {
            removed += 1;
            DiffLine::Removed(line.to_string())
        } else {
            DiffLine::Context(line.to_string())
        };
        lines.push(dl);
    }

    FileDiff {
        filename,
        added,
        removed,
        lines,
    }
}

/// Extract the filename from a `diff --git a/... b/...` line.
/// Falls back to the raw line if parsing fails.
fn extract_filename(header: &str) -> String {
    // Format: "diff --git a/path b/path"
    if let Some(rest) = header.strip_prefix("diff --git ") {
        // Split on " b/" — the last occurrence handles paths with spaces
        if let Some(pos) = rest.rfind(" b/") {
            return rest[pos + 3..].to_string();
        }
    }
    header.to_string()
}
