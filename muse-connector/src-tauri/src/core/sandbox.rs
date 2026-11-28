//! Command execution sandbox: platform dispatch.
//!
//! Windows (`sandbox/windows.rs`): CreateRestrictedToken WRITE_RESTRICTED +
//! per-workspace directory ACLs + Job Object. The kernel enforces the write
//! boundary; sandbox setup failure is a hard error, never a silent downgrade.
//!
//! macOS (`sandbox/macos.rs`): there is NO equivalent of a write-restricted
//! token or Job Object on macOS, and this port does not pretend otherwise.
//! The Mac backend confines a command to its authorized working directory
//! (cwd jail), strips the environment to a small allowlist, caps virtual
//! memory with `RLIMIT_AS`, and runs the child in its own process group so
//! `terminate()` kills the whole tree. The app's own three-tier approval
//! flow (auto-allow / session approval with a 120 s local dialog / forced
//! single confirmation) still gates every launch. See `sandbox/macos.rs` and
//! `MAC-PORT.md` for the full honest list of what the Mac sandbox does and
//! does NOT do.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub(crate) use macos::*;
#[cfg(windows)]
pub(crate) use windows::*;
