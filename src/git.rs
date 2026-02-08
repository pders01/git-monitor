use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};

use crate::diff::{self, DiffLine};

/// One entry from `git log`.
#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub date_relative: String,
}

/// Snapshot of everything we need from git to render one frame.
pub struct RepoState {
    pub branch: String,
    pub last_commit_hash: Option<String>,
    pub last_commit_message: Option<String>,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub unstaged_diff: Vec<DiffLine>,
    pub staged_diff: Vec<DiffLine>,
    pub refreshed_at: Instant,
}

impl RepoState {
    /// Build a complete snapshot by shelling out to git.
    ///
    /// Tolerant of empty repos (no commits yet) — falls back gracefully.
    pub fn query(repo: &Path) -> Result<Self> {
        let branch = git_branch(repo).unwrap_or_else(|_| "(no branch)".into());
        let (hash, msg) = git_last_commit(repo).unwrap_or((None, None));
        let (staged, unstaged) = git_status_counts(repo)?;
        let unstaged_raw = git_diff(repo, false).unwrap_or_default();
        let staged_raw = git_diff(repo, true).unwrap_or_default();

        Ok(Self {
            branch,
            last_commit_hash: hash,
            last_commit_message: msg,
            staged_count: staged,
            unstaged_count: unstaged,
            unstaged_diff: diff::parse(&unstaged_raw),
            staged_diff: diff::parse(&staged_raw),
            refreshed_at: Instant::now(),
        })
    }

    /// Return a fallback state for when the repo has no commits yet.
    pub fn empty(reason: &str) -> Self {
        Self {
            branch: String::from("(unknown)"),
            last_commit_hash: None,
            last_commit_message: None,
            staged_count: 0,
            unstaged_count: 0,
            unstaged_diff: vec![DiffLine::Context(reason.to_string())],
            staged_diff: vec![],
            refreshed_at: Instant::now(),
        }
    }
}

// ── helpers ─────────────────────────────────────────────────────

fn run_git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(["-C", &repo.to_string_lossy()])
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_branch(repo: &Path) -> Result<String> {
    // Works for both attached and detached HEAD
    let out = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = out.trim();
    if branch == "HEAD" {
        // Detached — show short SHA instead
        let sha = run_git(repo, &["rev-parse", "--short", "HEAD"])?;
        Ok(format!("detached:{}", sha.trim()))
    } else {
        Ok(branch.to_string())
    }
}

fn git_last_commit(repo: &Path) -> Result<(Option<String>, Option<String>)> {
    let out = run_git(repo, &["log", "-1", "--format=%H%n%s"])?;
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Ok((None, None));
    }
    let mut lines = trimmed.lines();
    let hash = lines.next().map(String::from);
    let msg = lines.next().map(String::from);
    Ok((hash, msg))
}

fn git_status_counts(repo: &Path) -> Result<(usize, usize)> {
    let out = run_git(repo, &["status", "--porcelain"])?;
    let mut staged = 0;
    let mut unstaged = 0;
    for line in out.lines() {
        if line.len() < 2 {
            continue;
        }
        let bytes = line.as_bytes();
        // First column: index (staged) status
        if bytes[0] != b' ' && bytes[0] != b'?' {
            staged += 1;
        }
        // Second column: working-tree (unstaged) status
        if bytes[1] != b' ' {
            unstaged += 1;
        }
    }
    Ok((staged, unstaged))
}

fn git_diff(repo: &Path, staged: bool) -> Result<String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    run_git(repo, &args)
}

/// Fetch recent commits as structured entries.
pub fn git_log(repo: &Path, count: usize) -> Result<Vec<CommitEntry>> {
    let count_str = format!("-{count}");
    let out = run_git(
        repo,
        &["log", "--format=%h%x00%s%x00%an%x00%ar", &count_str],
    )?;
    let mut entries = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.splitn(4, '\0').collect();
        if parts.len() == 4 {
            entries.push(CommitEntry {
                hash: parts[0].to_string(),
                message: parts[1].to_string(),
                author: parts[2].to_string(),
                date_relative: parts[3].to_string(),
            });
        }
    }
    Ok(entries)
}

/// Get the full output of `git show <hash>` for piping to an external pager.
pub fn git_show(repo: &Path, hash: &str) -> Result<String> {
    run_git(repo, &["show", hash])
}
