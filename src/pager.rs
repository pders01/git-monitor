use std::io::Write;
use std::process::{Command, Stdio};

/// Detect the user's preferred pager.
/// Checks GIT_PAGER -> git config core.pager -> PAGER -> "less"
pub fn detect_pager() -> String {
    if let Ok(pager) = std::env::var("GIT_PAGER") {
        if !pager.is_empty() {
            return pager;
        }
    }

    if let Ok(output) = Command::new("git").args(["config", "core.pager"]).output() {
        if output.status.success() {
            let pager = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !pager.is_empty() {
                return pager;
            }
        }
    }

    if let Ok(pager) = std::env::var("PAGER") {
        if !pager.is_empty() {
            return pager;
        }
    }

    "less".to_string()
}

/// Pipe content to the pager's stdin, the same way git does it.
pub fn open_pager(content: &str, pager_cmd: &str) -> std::io::Result<()> {
    let cmd = ensure_paging_always(pager_cmd);

    let mut child = Command::new("sh")
        .args(["-c", &cmd])
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes());
    }

    child.wait()?;
    Ok(())
}

/// If the pager command invokes delta without an explicit --paging flag,
/// append `--paging=always` so it always spawns its internal pager.
fn ensure_paging_always(pager_cmd: &str) -> String {
    let has_delta = pager_cmd
        .split_whitespace()
        .any(|tok| tok == "delta" || tok.ends_with("/delta"));

    if has_delta && !pager_cmd.contains("--paging") {
        format!("{pager_cmd} --paging=always")
    } else {
        pager_cmd.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_delta_gets_paging_always() {
        assert_eq!(ensure_paging_always("delta"), "delta --paging=always");
    }

    #[test]
    fn delta_with_args_gets_paging_always() {
        assert_eq!(
            ensure_paging_always("delta --dark --side-by-side"),
            "delta --dark --side-by-side --paging=always"
        );
    }

    #[test]
    fn delta_with_explicit_paging_unchanged() {
        let cmd = "delta --paging=never";
        assert_eq!(ensure_paging_always(cmd), cmd);
    }

    #[test]
    fn less_unchanged() {
        assert_eq!(ensure_paging_always("less"), "less");
    }

    #[test]
    fn delta_in_unrelated_word_unchanged() {
        assert_eq!(ensure_paging_always("deltaforce"), "deltaforce");
    }
}
