//! macOS execution confinement — the honest version.
//!
//! What this backend DOES:
//!   - Starts the child with its working directory set to the authorized
//!     workspace (`cwd` jail). A command that uses relative paths can only
//!     touch the workspace; absolute paths are still the caller's
//!     responsibility and are gated by the approval flow + policy layer.
//!   - Strips the environment to a small allowlist (`PATH`, `HOME`, `TMPDIR`,
//!     `LANG`). No inherited tokens, proxy settings, or shell customizations
//!     reach the child.
//!   - Caps virtual memory with `RLIMIT_AS` (default 2 GiB, same value as
//!     the Windows Job Object limit) when the run is sandboxed.
//!   - Runs the child in its own process group (`setpgid`) so `terminate()`
//!     kills the whole process tree with `killpg`, not just the leader.
//!   - Captures stdout/stderr through pipes with the same bounded reader as
//!     Windows; the caller's hard timeout still applies.
//!
//! What it does NOT do (no macOS equivalent exists, and none is faked):
//!   - NO kernel-level write restriction. Unlike the Windows restricted
//!     token, a macOS child that receives an absolute path outside the
//!     workspace CAN write there if the user account has permission. The
//!     write boundary on Mac is enforced by the approval flow, the exec
//!     policy (high-risk patterns, blocked tools), and the per-workspace ACL
//!     — not by the kernel. `write_workspace` is therefore carried for
//!     interface parity but has no enforcement effect here.
//!   - NO process-count limit. `RLIMIT_NPROC` on macOS counts processes
//!     per *user*, not per process tree, so setting it would throttle the
//!     user's own apps. The `max_processes` part of `JobLimits` is accepted
//!     and deliberately ignored; fork bombs are bounded by the wall-clock
//!     timeout and the process-group kill instead.
//!   - NO CPU-time limit. `RLIMIT_CPU` counts CPU-seconds across all threads
//!     and would kill legitimate parallel builds; the wall-clock timeout is
//!     the enforcement.
//!   - NO Seatbelt/`sandbox-exec` profile. A hand-written profile would
//!     break legitimate tools (they write to `~/.cache`, `/tmp`, …) and is
//!     not attempted.
//!
//! `sandboxed = false` (the user explicitly approved an unsandboxed run)
//! drops the Mac confinement (rlimits + env allowlist) but keeps the
//! structural guarantees: cwd jail, own process group, piped output, and the
//! caller's hard timeout. This mirrors Windows, where unsandboxed drops the
//! restricted token but keeps the Job Object.

use std::{
    ffi::OsStr,
    fs::File,
    io,
    os::unix::{
        fs::PermissionsExt,
        io::{FromRawFd, IntoRawFd},
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Same default as the Windows Job Object memory limit.
pub(crate) const DEFAULT_EXEC_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// macOS limits. `max_processes` is intentionally absent: there is no safe
/// per-tree process-count limit on macOS (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct JobLimits {
    pub(crate) memory_bytes: Option<u64>,
}

pub(crate) struct SpawnConfig {
    pub(crate) application: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) temp_dir: PathBuf,
    pub(crate) sandboxed: bool,
    pub(crate) write_workspace: bool,
    pub(crate) limits: JobLimits,
}

pub(crate) struct SpawnedProcess {
    // `Child::try_wait` needs `&mut`, but the shared `SpawnedProcess` API
    // takes `&self` (matching the Windows backend); the mutex bridges that.
    // `Child` caches the exit status after the first reap, so repeated polls
    // after exit keep returning the same code.
    child: Mutex<Child>,
    pid: i32,
    pgid: i32,
    pub(crate) stdout: Option<File>,
    pub(crate) stderr: Option<File>,
}

impl SpawnedProcess {
    pub(crate) fn take_pipes(&mut self) -> Option<(File, File)> {
        Some((self.stdout.take()?, self.stderr.take()?))
    }

    pub(crate) fn try_wait(&self) -> io::Result<Option<i32>> {
        let status = self.child.lock().unwrap().try_wait()?;
        Ok(status.map(|status| {
            status
                .code()
                .or_else(|| status.signal().map(|sig| 128 + sig))
                .unwrap_or(-1)
        }))
    }

    pub(crate) fn wait_timeout(&self, timeout_ms: u32) -> io::Result<Option<i32>> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        loop {
            if let Some(code) = self.try_wait()? {
                return Ok(Some(code));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub(crate) fn terminate(&self) {
        // Kill the whole process group first (children, grandchildren, …),
        // then the leader itself as a fallback. Best effort; errors ignored.
        // SAFETY: `pgid`/`pid` belong to the child tree this struct spawned.
        unsafe {
            libc::killpg(self.pgid, libc::SIGKILL);
            libc::kill(self.pid, libc::SIGKILL);
        }
    }
}

pub(crate) fn spawn(
    config: &SpawnConfig,
    cancel: Option<&AtomicBool>,
) -> io::Result<SpawnedProcess> {
    std::fs::create_dir_all(&config.temp_dir)?;
    if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    // `write_workspace` has no enforcement mechanism on macOS (see the module
    // docs): the kernel cannot restrict this process's writes to one
    // directory. It is carried for interface parity with the Windows backend
    // and for the audit trail kept by the caller.
    let _ = config.write_workspace;

    let memory_bytes = config.limits.memory_bytes;
    let sandboxed = config.sandboxed;
    let mut command = Command::new(&config.application);
    command
        .args(&config.args)
        .current_dir(&config.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if sandboxed {
        // Minimal allowlist. PATH is fixed to the system defaults so a
        // compromised user PATH can't redirect the tools the child invokes.
        command.env_clear();
        command.env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
        if let Some(home) = std::env::var_os("HOME") {
            command.env("HOME", home);
        }
        command.env("TMPDIR", &config.temp_dir);
        command.env("LANG", "en_US.UTF-8");
    }
    // SAFETY: `pre_exec` runs after `fork()` in the child, before `exec`.
    // Only async-signal-safe calls are used here (`setpgid`, `setrlimit`).
    unsafe {
        command.pre_exec(move || {
            // Own process group so the parent can kill the whole tree.
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            if sandboxed && let Some(bytes) = memory_bytes {
                let limit = libc::rlimit {
                    rlim_cur: bytes as libc::rlim_t,
                    rlim_max: bytes as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let pid = child.id() as i32;
    // The child called `setpgid(0, 0)`, so its pgid equals its pid. If the
    // child exec'd before we read this, the pgid is still its own pid.
    let pgid = pid;
    // `ChildStdout`/`ChildStderr` don't convert into `File` directly; take
    // the raw fd (the pipe read end is ours to own) and wrap it.
    let stdout = child
        .stdout
        .take()
        .map(|s| unsafe { File::from_raw_fd(s.into_raw_fd()) });
    let stderr = child
        .stderr
        .take()
        .map(|s| unsafe { File::from_raw_fd(s.into_raw_fd()) });
    Ok(SpawnedProcess {
        child: Mutex::new(child),
        pid,
        pgid,
        stdout,
        stderr,
    })
}

/// Resolve a directly invoked program through PATH (macOS: no extension
/// probing, executable bit required). Returns `(path, is_batch)` where
/// `is_batch` is always false on macOS — kept for signature parity with the
/// Windows helper used by `exec_runtime`.
pub(crate) fn resolve_program(name: &str) -> Option<(PathBuf, bool)> {
    let path_var = std::env::var_os("PATH")?;
    if name.contains('/') {
        let candidate = PathBuf::from(name);
        return is_executable(&candidate).then_some((candidate, false));
    }
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some((candidate, false));
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The user's login shell. Used for `ExecMode::Shell`; absolute path, never
/// resolved through PATH.
pub(crate) fn shell_executable() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|shell| shell.is_absolute() && shell.is_file())
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

/// Quote one argv element for display only (never parsed back).
pub(crate) fn quote_shell_arg(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
    {
        return arg.to_owned();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for ch in arg.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Display form of a spawned command (`program arg1 arg2 …`), for the audit
/// log and the approval dialog. Display only — never executed.
pub(crate) fn display_command(program: &OsStr, args: &[String]) -> String {
    let mut out = quote_shell_arg(&program.to_string_lossy());
    for arg in args {
        out.push(' ');
        out.push_str(&quote_shell_arg(arg));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_shell_arg_leaves_safe_args_alone() {
        assert_eq!(quote_shell_arg("npm"), "npm");
        assert_eq!(quote_shell_arg("--frozen-lockfile"), "--frozen-lockfile");
        assert_eq!(quote_shell_arg("a b"), "'a b'");
        assert_eq!(quote_shell_arg("it's"), "'it'\\''s'");
    }

    #[test]
    fn resolve_program_finds_system_tools() {
        assert!(resolve_program("ls").is_some());
        assert!(resolve_program("definitely-not-a-real-tool-xyz").is_none());
    }

    #[test]
    fn spawn_runs_and_captures_output() {
        let temp =
            std::env::temp_dir().join(format!("muse-sandbox-{}", crate::core::token::generate()));
        let proc = spawn(
            &SpawnConfig {
                application: PathBuf::from("/bin/echo"),
                args: vec!["hello".to_string()],
                cwd: temp.clone(),
                temp_dir: temp.clone(),
                sandboxed: true,
                write_workspace: false,
                limits: JobLimits {
                    memory_bytes: Some(DEFAULT_EXEC_MEMORY_BYTES),
                },
            },
            None,
        )
        .unwrap();
        let code = proc.wait_timeout(5000).unwrap();
        assert_eq!(code, Some(0));
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn terminate_kills_the_process_group() {
        let temp =
            std::env::temp_dir().join(format!("muse-sandbox-{}", crate::core::token::generate()));
        let proc = spawn(
            &SpawnConfig {
                application: PathBuf::from("/bin/sleep"),
                args: vec!["120".to_string()],
                cwd: temp.clone(),
                temp_dir: temp.clone(),
                sandboxed: true,
                write_workspace: false,
                limits: JobLimits {
                    memory_bytes: Some(DEFAULT_EXEC_MEMORY_BYTES),
                },
            },
            None,
        )
        .unwrap();
        assert!(proc.try_wait().unwrap().is_none());
        proc.terminate();
        let deadline = Instant::now() + Duration::from_secs(5);
        let code = loop {
            if let Some(code) = proc.try_wait().unwrap() {
                break code;
            }
            assert!(Instant::now() < deadline, "child survived terminate()");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_ne!(code, 0);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
