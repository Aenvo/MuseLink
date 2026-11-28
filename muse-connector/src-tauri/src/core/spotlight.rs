// Spotlight search backend (macOS only).
//
// Mirrors the Everything client's API surface (`discover`, `query`,
// `Candidates`, `stop_managed`) so the search layer in `search.rs` /
// `access_runtime.rs` doesn't change shape. The fast path is the system
// `mdfind` CLI, which queries the Spotlight index; any scope Spotlight
// doesn't index is covered by the built-in Rust directory walker in
// `search.rs` (see `SearchIndex::search`), which runs whenever this backend
// is unavailable or its candidate budget is exhausted.
//
// What Spotlight can and can't do, honestly:
//   - `mdfind -onlyin <dir>` is root-scoped, so results are confined to the
//     authorized scope by construction. Queries use `kMDItemFSName`, so the
//     match is on file *names* only — the same contract as the Everything
//     backend's `name:glob` search.
//   - Spotlight never indexes some locations (system dirs, hidden dirs such
//     as `~/Library`, volumes with indexing disabled, and paths excluded in
//     System Settings → Spotlight → Privacy). Those paths are still found by
//     the built-in walker; they are just slower, and on first use the walker
//     builds its index in the background.
//   - There is no managed instance to start or stop (`stop_managed` is a
//     no-op), and there is no equivalent of Everything's NTFS journal scan.
//     Freshly created files may take a few seconds to appear in the index.
//   - `mdfind` is a user-space CLI; it runs as the same user, needs no
//     entitlement, and reads no file contents — only indexed names/paths.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
/// Mirrors Everything's `MANAGED_CANDIDATE_LIMIT`: at the cap the answer is
/// marked uncovered so the caller falls back to the built-in index, which can
/// page past the budget.
const CANDIDATE_LIMIT: usize = 2048;

pub(super) struct Spotlight;

pub(super) struct Candidates {
    pub paths: Vec<PathBuf>,
    pub scanning: bool,
    pub covered: bool,
}

impl Spotlight {
    pub(super) fn discover(_app_home: &Path) -> Option<Self> {
        if std::env::var_os("MUSE_CONNECTOR_DISABLE_SPOTLIGHT").is_some() {
            return None;
        }
        // `mdfind` ships at a fixed path on every macOS. If it is gone, the
        // caller degrades to the built-in index instead of erroring per query.
        if !Path::new("/usr/bin/mdfind").is_file() {
            return None;
        }
        Some(Self)
    }

    pub(super) fn query(
        &self,
        roots: &[PathBuf],
        _authorized_roots: &[PathBuf],
        name: &str,
        cancel: &AtomicBool,
    ) -> Option<Candidates> {
        if roots.is_empty() || roots.len() > 32 || cancel.load(Ordering::Acquire) {
            return None;
        }
        let mut command = Command::new("mdfind");
        for root in roots {
            command.arg("-onlyin").arg(root);
        }
        // Name match only, case-insensitive, substring. The glob lives inside
        // the query string so no shell ever sees it; quote/backslash in the
        // name are escaped so a crafted filename can't break the query.
        command.arg(format!("kMDItemFSName == \"*{}*\"cd", escape_query(name)));
        command.arg("-0");
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // std has no `Child::wait_timeout` (that method lives in the
        // `wait-timeout` crate, which this crate doesn't depend on), so poll
        // `try_wait` until the deadline, then kill the child.
        let deadline = std::time::Instant::now() + QUERY_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
        if cancel.load(Ordering::Acquire) {
            return None;
        }
        let mut raw = Vec::new();
        child.stdout.take()?.read_to_end(&mut raw).ok()?;
        let mut paths: Vec<PathBuf> = Vec::new();
        for part in raw.split(|&b| b == 0) {
            if part.is_empty() {
                continue;
            }
            let path = PathBuf::from(String::from_utf8_lossy(part).into_owned());
            // Belt and braces: `-onlyin` already scopes the query, but a
            // Spotlight bug or a future CLI change must never leak a path
            // outside the authorized scope.
            if roots.iter().any(|root| path.starts_with(root)) {
                paths.push(path);
                if paths.len() >= CANDIDATE_LIMIT {
                    break;
                }
            }
        }
        let covered = paths.len() < CANDIDATE_LIMIT;
        Some(Candidates {
            paths,
            scanning: false,
            covered,
        })
    }

    /// No-op on macOS: there is no managed Spotlight instance to stop.
    pub(super) fn stop_managed(&self) {}
}

/// Escape a filename fragment for embedding in an `mdfind` query string.
fn escape_query(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_escaping_neutralizes_quotes() {
        assert_eq!(escape_query("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape_query("plain"), "plain");
    }

    #[test]
    fn discover_respects_disable_env() {
        // SAFETY: single-threaded test process; the var is restored after.
        let previous = std::env::var_os("MUSE_CONNECTOR_DISABLE_SPOTLIGHT");
        unsafe { std::env::set_var("MUSE_CONNECTOR_DISABLE_SPOTLIGHT", "1") };
        assert!(Spotlight::discover(Path::new("/tmp")).is_none());
        match previous {
            Some(value) => unsafe { std::env::set_var("MUSE_CONNECTOR_DISABLE_SPOTLIGHT", value) },
            None => unsafe { std::env::remove_var("MUSE_CONNECTOR_DISABLE_SPOTLIGHT") },
        }
    }
}
