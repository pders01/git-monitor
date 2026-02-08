use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::Result;
use ignore::gitignore::Gitignore;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::event::AppEvent;

/// Start the filesystem watcher in its own thread.
///
/// Returns a handle that keeps the watcher alive — dropping it stops watching.
pub fn spawn(
    repo: &Path,
    debounce_ms: u64,
    tx: Sender<AppEvent>,
) -> Result<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    let repo_path = repo.to_path_buf();
    let git_dir = repo.join(".git");

    // Build the gitignore matcher from the repo's .gitignore (if any)
    let gitignore_path = repo.join(".gitignore");
    let (gitignore, _) = Gitignore::new(&gitignore_path);

    let mut debouncer = new_debouncer(
        Duration::from_millis(debounce_ms),
        move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            let events = match res {
                Ok(evts) => evts,
                Err(_) => return,
            };

            for event in &events {
                if event.kind != DebouncedEventKind::Any {
                    continue;
                }
                if should_notify(&event.path, &repo_path, &git_dir, &gitignore) {
                    let _ = tx.send(AppEvent::FsChange);
                    return; // one FsChange per debounce batch is enough
                }
            }
        },
    )?;

    debouncer
        .watcher()
        .watch(repo.as_ref(), notify::RecursiveMode::Recursive)?;

    Ok(debouncer)
}

/// Decide whether a filesystem event path should trigger a refresh.
fn should_notify(path: &Path, repo: &Path, git_dir: &PathBuf, gitignore: &Gitignore) -> bool {
    // Inside .git/ — only care about specific paths that indicate state changes
    if path.starts_with(git_dir) {
        return is_interesting_git_path(path, git_dir);
    }

    // Working tree file — check gitignore
    if let Ok(relative) = path.strip_prefix(repo) {
        let is_dir = path.metadata().map(|m| m.is_dir()).unwrap_or(false);
        return !gitignore.matched(relative, is_dir).is_ignore();
    }

    // Path outside repo — ignore
    false
}

/// Within `.git/`, only a few paths signal meaningful state changes.
fn is_interesting_git_path(path: &Path, git_dir: &PathBuf) -> bool {
    if let Ok(relative) = path.strip_prefix(git_dir) {
        let rel_str = relative.to_string_lossy();
        // index = staging area changes
        // HEAD = branch switch / commit
        // refs/ = new commits, branch/tag creation
        // MERGE_HEAD, REBASE_HEAD = merge/rebase state
        rel_str == "index"
            || rel_str == "HEAD"
            || rel_str.starts_with("refs/")
            || rel_str == "MERGE_HEAD"
            || rel_str == "REBASE_HEAD"
    } else {
        false
    }
}
