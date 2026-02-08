/// A single line from a unified diff, classified by type.
#[derive(Debug, Clone)]
pub enum DiffLine {
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
            DiffLine::Header(s)
            | DiffLine::Hunk(s)
            | DiffLine::Added(s)
            | DiffLine::Removed(s)
            | DiffLine::Context(s) => s,
        }
    }
}

/// Parse the raw output of `git diff` into typed diff lines.
///
/// Note: `--- a/` and `+++ b/` are matched as headers *before* checking
/// for single `+`/`-` prefixes, so they don't get misclassified.
pub fn parse(raw: &str) -> Vec<DiffLine> {
    raw.lines()
        .map(|line| {
            if line.starts_with("diff --git ")
                || line.starts_with("index ")
                || line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("Binary files ")
            {
                DiffLine::Header(line.to_string())
            } else if line.starts_with("@@") {
                DiffLine::Hunk(line.to_string())
            } else if line.starts_with('+') {
                DiffLine::Added(line.to_string())
            } else if line.starts_with('-') {
                DiffLine::Removed(line.to_string())
            } else {
                DiffLine::Context(line.to_string())
            }
        })
        .collect()
}
