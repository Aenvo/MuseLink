# Changelog

[简体中文](CHANGELOG.md) · [English](CHANGELOG.en.md) · [Back to README](README.en.md)

This changelog tracks feature updates, security hardening, and bug fixes for MuseLink, kept in sync with repository pushes and releases.

## [v0.2.1] - 2026-11-25

### Improvements & Added
- **Removed Cloudflare quick tunnel**: Removed the `trycloudflare.com` quick tunnel option from both UI and Rust backend after confirming it is unreliable with Muse; MuseLink now focuses on Tailscale Funnel and Cloudflare named tunnels (with automatic fallback to Tailscale Funnel when loading legacy `cloudflare_quick` configs).
- **Real-time long-polling for command approvals and execution status**: `GET /agent/v1/exec/{job_id}` now supports an optional `?wait_ms=` query parameter (defaults to `0` for full compatibility, capped at `25000` ms, recommended `20000` ms). Waiting runs asynchronously without holding request slots or access locks, waking immediately when you click **Allow** or **Deny**, when approval expires, or when the job finishes—no need to reply in chat after approving on Windows.
- **Updated Muse connection instructions (Prompt)**: Updated both English and Chinese connector instructions so Muse keeps polling `GET /agent/v1/exec/<job_id>?wait_ms=20000` continuously until a terminal state instead of stopping for a manual reply, with clear handling for `expired` / `denied` and bounded polling caps (existing users can re-copy and send the updated instructions to Muse).
- **Prevent stale clicks at approval expiry**: Disables approval dialog buttons once the countdown reaches 0 seconds.
- **Filterable log export & log folder shortcut**: Added an **Export logs** dialog and **Open log folder** button to the Logs page. Filter by log scope (All / Files / Command execution / All access / Runtime), result, time range, and keyword, then export chronologically as `JSONL` or `CSV` (with UTF-8 BOM and redundant token redaction) through the native save dialog, or jump straight to the rotated 10 MiB log files on disk.
- **Start in system tray enabled by default**: New profiles default to **Settings → Start in the system tray** enabled (the main window still opens normally during first-time setup).
- **MIT open-source license & notices**: Added repository-root [LICENSE](LICENSE) (MIT License), synced `package.json`, `Cargo.toml`, bilingual READMEs, third-party notices, and bundled `LICENSE` in release packages.

### Fixes
- **Fixed jobs hanging in `running` when a detached grandchild holds stdio pipes**: Terminates and drops the per-job Windows `JobObject` right after the primary command exits and before joining the stdout/stderr reader threads, preventing inherited pipe write handles from blocking EOF indefinitely.

## [v0.2.0] - 2026-11-25

### Added
- **Controlled local command execution (off by default)**: Added a toggle with risk confirmation under **File Access → Muse file tools**. When enabled, Muse can run developer tools (`direct` mode) and controlled `powershell` / `cmd` commands inside authorized project subfolders via `/agent/v1/exec`, `/agent/v1/exec/{job_id}`, and `/agent/v1/exec/{job_id}/cancel` (bare drive roots are rejected).
- **Per-workspace Windows write-restricted sandbox**: Uses `CreateRestrictedToken(WRITE_RESTRICTED)` with a unique SHA-256-derived SID per canonical workspace path so writes are restricted strictly to the targeted project subfolder and an isolated per-job temp directory (sibling and parent folders cannot write into each other). Enforces `JobObject` memory, process-count, and timeout limits and fails closed without falling back to unsandboxed execution.
- **3-tier risk model & auto-elevated approval dialog**:
  - Standard read-only and project build/test commands run automatically and return HTTP 200 directly when fast; pure read-only direct commands skip workspace write ACL updates completely.
  - First-time shell usage, dependency mutations, or `git push` in a project wakes and temporarily elevates a local approval dialog (120-second timeout defaults to deny) with **Allow once** or 4-hour per-project **Allow project this session** (revocable anytime under **File Access → Revoke session grants**).
  - Sensitive paths, parent-directory traversal (`..`), force push, recursive/forced deletion, system admin tools, obfuscated scripts, and `unsandboxed` requests always require a single confirmation.
- **Device naming & rename receipts**: Configure distinct device names under **Connection** for multi-computer setups, with copyable rename instructions and Muse confirmation receipts.
- **Execution log filter & in-app Changelog button**: Added **Command execution** filter to the Logs page and a **Changelog** button next to **Open GitHub** on the **About** page.

### Security & Stability
- **Handle-verified file operations**: Reads and atomic writes verify real object paths and attributes through open Windows handles, supporting Windows short-path aliases while defending against symlink/junction escapes and TOCTOU swaps.
- **Layered sensitive-path protection**: Excludes credential directories (`~/.muse-bridge`, `~/.ssh`, `~/.aws`, `~/.gnupg`) by default and write-protects system directories, Startup folders, shell profiles, `.gitconfig`, and the app directory.
- **Clean child process-tree teardown**: Attaches managed `Everything.exe`, `cloudflared`, and command child processes to Windows `JobObject`s so exiting or disabling execution terminates entire process trees with zero orphans.
- **Log redaction & permission preservation**: Automatically redacts `mfa_` tokens in audit/runtime logs; includes `/agent/v1/*` endpoints in default **Files** logs; preserves existing grants when switching between folder and drive modes; enforces the 10 MiB limit on base64 writes; tightens CSP; bumps `url` to address `RUSTSEC-2026-0592`.
- **Structured disabled error code**: Returns HTTP 403 with `{"error": "exec_disabled", "message": "command execution is disabled"}` when command execution is disabled.

### Engineering & CI
- Added Windows GitHub Actions build and test workflow covering formatting, frontend build, Rust tests, symlink/junction escape coverage, and real Windows kernel sandbox tests.

## [v0.1.0] - 2026-11-24

### Initial Release
- **Guided setup & file permissions**: Authorize specific folders or local fixed drives with background Everything filename indexing and on-demand index rebuild.
- **Dedicated Muse token & file API**: Separate `/agent/v1` endpoints and dedicated `agent-token` (issue, copy, reveal, rotate, revoke) supporting search, list, read, write, and mkdir.
- **Public tunnels & tray controls**: Tailscale Funnel by default plus Cloudflare quick and named tunnels, tray start/stop controls, access stats and logs, and English / Simplified Chinese UI.
- **Windows distribution**: Portable ZIP archive, NSIS installer, and `SHA256SUMS.txt`.
