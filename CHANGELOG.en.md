# Changelog

[简体中文](CHANGELOG.md) · [English](CHANGELOG.en.md) · [日本語](CHANGELOG.ja.md) · [Back to README](README.en.md)

This changelog tracks feature updates, security hardening, and bug fixes for MuseLink, kept in sync with repository pushes and releases.

## [Unreleased]

### Improvements & Added
- **Layered in-app changelog reading & native Japanese changelog**: The in-app **About → Changelog / Release notes** dialog now shows concise highlights by default with a per-version toggle for technical details, and the Japanese UI displays native Japanese notes directly.
  - Added `CHANGELOG.ja.md` so the Japanese in-app dialog and GitHub links no longer fall back to English.
  - Upgraded the CI and release workflows to auto-discover all `CHANGELOG*.md` and `README*.md` locales, enforce cross-locale version sync, and assemble multilingual GitHub Release notes automatically when new languages are added.

### Security & Stability
- **Expanded default credential protection**: Added default read, search, and write protection for common cloud/container credential folders and browser login stores—even under full-disk or home-folder grants.
  - Promoted `.npmrc` and `.pypirc` from write-only protection to full protection, and added `.docker`, `.kube`, `.azure`, and gcloud credential directories.
  - Added per-platform browser credential store protection: Windows (Chrome / Edge `User Data`, Firefox `Profiles`), macOS (Chrome / Edge / Firefox under Application Support alongside existing `~/Library/Keychains`), and Linux (`google-chrome` / `chromium` / `microsoft-edge`, `.mozilla/firefox`).

## [v0.3.1] - 2026-11-27

### Fixes & Cross-Platform Adaptation
- **Fixed "Copy dedicated token" failures across all platforms**: Implemented native secret clipboard support on macOS, system clipboard tools on Linux, contention retries on Windows, and actionable failure diagnostics.
  - **Native macOS secret pasteboard**: Copies tokens via AppKit `NSPasteboard` with `org.nspasteboard.ConcealedType`, `org.nspasteboard.TransientType`, and `CurrentHostOnly` to keep credentials out of clipboard managers and Universal Clipboard sync.
  - **Linux clipboard support**: Writes via stdin to `wl-copy`, `xclip`, or `xsel` without passing tokens on the command line and detaches stdio so background clipboard daemons never hang.
  - **Windows clipboard retry & privacy flags**: Added short backoff retries around `OpenClipboard` to mitigate transient contention and standardized 4-byte `DWORD(0)` markers (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`, `CanUploadClipboardData`).
  - **Actionable failure diagnostics**: Surfaced localized failure reasons (ZH/EN/JA) in the setup guide and connection card while keeping the **Locate token file** fallback.
- **Fixed macOS / Linux "Manage local drives (full access)" returning HTTP 403 for Desktop and all requests**: Added system root (`/`) discovery, Unix sensitive-path protection, and macOS privacy guidance.
  - **Unix local root discovery & validation**: Implemented `fixed_local_roots()` on macOS and Linux to expose `root` -> `/`, updated disk alias validation and the disk picker UI (`System root (/)`), and failed closed on empty root maps—fixing empty `/agent/v1/roots` and HTTP 403 on `/agent/v1/locations/desktop`.
  - **macOS / Linux sensitive-path protection & safe search exclusions**: Added full protection for macOS `~/Library/Keychains` and default write protection for system directories, autostart paths (`~/Library/LaunchAgents`, `~/.config/autostart`), login shell profiles (`.zprofile`, `.zshenv`, `.bash_profile`), and `.app` bundles; excluded macOS `/System`, `/Library`, `/Applications`, `/usr`, `/bin`, `/sbin`, `/private`, `~/Library` (preventing delayed iCloud / app-container TCC prompt storms), Photos library, `/dev`, and non-local `/Volumes/*` mounts (and Linux `/proc`, `/sys`, `/dev`, `/run`) from background scanning without blocking direct reads.
  - **macOS privacy permission (TCC) UX & API error clarity**: Primed Desktop, Documents, and Downloads upfront before recursive indexing; added a one-click **Open Full Disk Access settings** button on macOS; made trilingual Desktop wording platform-neutral; and mapped OS `PermissionDenied` errors to actionable HTTP 403 responses.

## [v0.3.0] - 2026-11-27

### Added
- **In-app auto-updater & synced changelog dialog**: Supports silent startup checks, signed one-click updates (Windows installer & Linux AppImage), and in-app synced release notes.
  - **Check & restart-to-update**: Uses static GitHub Releases `latest.json` manifests and a compile-time pinned Minisign public key (no rate-limited REST API); shows a dismissible bottom-right update card with a live green progress bar while keeping connections active, cleanly stops services (including Windows `Everything.exe`) before restarting; portable Windows, Linux `.deb`, and macOS/source builds receive alerts, notes, and direct download links via the same manifest.
  - **In-app changelog modals**: Updates **About** actions to **Changelog**, **GitHub**, and **Check for updates**; renders instantly from bundled notes, silently syncs from GitHub, follows UI language, and links directly to GitHub.
  - **Cross-platform automated release workflow**: Added `.github/workflows/release.yml` and `generate-updater-manifest.ps1` to build and verify all three platforms in parallel on `v*` tags and publish `latest.json` and `SHA256SUMS.txt`.
- **Linux & macOS (Apple Silicon & Intel) support**: Full file API and Tailscale / Cloudflare tunnel configuration now run across all three desktop platforms with prebuilt packages.
  - **Linux support**: Covers the file API (`search` / `list` / `read` / `write` / `mkdir`) and tunnel setup with signed AppImage and `.deb` packages; sandboxed command execution remains Windows-only.
  - **macOS support**: Provides Universal `.dmg` and `.app.zip` packages and discovers `Tailscale.app` and Homebrew installs automatically; packages are ad-hoc signed and not notarized (allow in **System Settings → Privacy & Security** on first launch); command execution remains Windows-only.
- **Japanese (`ja`) localization**: Added complete Japanese UI, system tray, Muse setup instructions, and Japanese README, selectable under **Settings → General**.

## [v0.2.2] - 2026-11-27

### Improvements & Added
- **Higher file read limit, chunked reading, and raw binary streaming (fixes #4)**: Raised whole-file reads to 10 MiB and added chunked and zero-overhead binary downloads for large files and high-resolution images.
  - **10 MiB whole-file JSON limit**: Matches the write limit so existing connectors can read large photos without prompt changes; oversized files return HTTP `413` with chunking and `raw` hints; legacy `/api/read` keeps 2 MiB compatibility.
  - **Chunked reads via `offset` / `limit` (no hard total size cap)**: Seeks and reads directly from the verified OS handle (JSON default 2 MiB, max 4 MiB), returns `etag`, and validates `if_match` / `If-Match` (returning HTTP `409` on concurrent edits).
  - **Raw binary streaming (`raw=1`)**: Streams raw bytes with detected `Content-Type` without 33% Base64 overhead (~40% faster over tunnels, up to 32 MiB per request, combinable with `offset`/`limit`), exposing `ETag` and `X-File-*` headers.
  - **Updated Muse connection instructions**: Guides Muse to prefer `raw=1` for binary/image files and retry individual chunks on transient drops.

## [v0.2.1] - 2026-11-25

### Improvements & Added
- **Full UI/UX redesign & information architecture refresh**: Unified visual polish and streamlined the Connection, File Access, Logs, and Settings pages.
  - **Component polish**: Active `Switch` track updated to `#48C52C` green with white thumb; `Input`, `Select`, and endpoint boxes unified as transparent outlines; inline search icon on Logs; semantic red for destructive actions.
  - **Connection & setup guide overhaul**: Dedicated top **Setup guide** card, inline **Device** row, **Muse connection instructions** moved to the Connection page, and side-by-side copy actions in step 3 of onboarding.
  - **Focused File Access & Logs**: Moved **Save permissions** into the card header; promoted time-window filtering into a single primary filter bar with a single **Pause sync / Resume sync** toggle.
  - **Settings reorganized into General & Data**: Consolidated local data, file index, logs, and token maintenance under **Data**.
  - **Improved startup & tray behavior**: Autostart defaults off and starts minimized to tray via `--autostart`; manual launches always open the main window; added configurable **Minimize to tray on close** (on by default).
- **Real-time long-polling for command approvals and execution**: Approvals, denials, and completions wake immediately without requiring a manual reply in chat.
  - Added `?wait_ms=` to `GET /agent/v1/exec/{job_id}` (default `0`, cap `25000` ms, recommended `20000` ms) running asynchronously without holding request slots or locks.
  - Updated Muse instructions for continuous polling with `expired`/`denied` handling and polling caps; disabled approval buttons once the countdown hits zero.
- **Filterable log export & log folder shortcut**: Export redacted logs as `JSONL` or `CSV` by scope, result, time range, and keyword, or open the rotated log directory directly.
  - Available under **Settings → Data → Log data** with **Clear logs**, **Export logs** (UTF-8 BOM + redundant token redaction), and **Open folder** (10 MiB rotated logs).
- **Removed Cloudflare quick tunnel & added MIT license**: Focused tunnel support on Tailscale Funnel and Cloudflare named tunnels.
  - Legacy `cloudflare_quick` configs fall back automatically to Tailscale Funnel; added repository-root [LICENSE](LICENSE) (MIT) synced across manifests, READMEs, and release packages.

### Fixes
- **Fixed jobs hanging in `running` when a detached grandchild holds stdio pipes**: Terminates the per-job Windows `JobObject` right after the primary command exits so inherited pipe handles cannot block EOF indefinitely.

## [v0.2.0] - 2026-11-25

### Added
- **Controlled local command execution (Windows, off by default)**: Allows Muse to run developer tools and controlled shells inside authorized project subfolders under a write-restricted sandbox and local approval dialog.
  - Added toggle with risk confirmation under **File Access → Muse file tools** exposing `/agent/v1/exec`, status polling, and cancellation (bare drive roots rejected).
  - **Per-workspace Windows write-restricted sandbox**: Combines `CreateRestrictedToken(WRITE_RESTRICTED)` with a SHA-256-derived SID per canonical workspace so writes stay within the project subfolder and isolated job temp dir, enforced alongside `JobObject` memory/process/timeout limits and fail-closed guarantees.
  - **3-tier risk model & auto-elevated approval dialog**: Read-only and standard build/test commands run automatically; first-time shell usage, dependency changes, or `git push` elevate a 120-second approval dialog (**Allow once** or 4-hour per-project session grant); sensitive paths, `..`, force push, recursive deletion, admin tools, obfuscated scripts, and `unsandboxed` requests always require single confirmation.
- **Device naming & rename receipts**: Configure distinct device names under **Connection** with copyable rename instructions and Muse confirmation receipts.
- **Execution log filter & in-app Changelog button**: Added **Command execution** log filtering and a **Changelog** button on **About**.

### Security & Stability
- **Handle-verified file operations & layered sensitive-path protection**: Reads and atomic writes verify real object paths via open OS handles against symlink/junction escapes and TOCTOU swaps, while protecting credentials and system paths by default.
  - Excludes `~/.muse-bridge`, `~/.ssh`, `~/.aws`, and `~/.gnupg` by default and write-protects system folders, Startup, shell profiles, `.gitconfig`, and the app directory.
  - Attaches managed `Everything.exe`, `cloudflared`, and command child processes to Windows `JobObject`s for clean process-tree teardown.
  - Redacts `mfa_` tokens in logs, preserves grants across folder/drive mode switches, enforces 10 MiB base64 write caps, tightens CSP, bumps `url` for `RUSTSEC-2026-0592`, and returns HTTP 403 `exec_disabled` when execution is off.

### Engineering & CI
- **Added Windows GitHub Actions CI**: Covers formatting, frontend build, Rust unit tests, symlink/junction escape checks, and real Windows kernel sandbox tests.

## [v0.1.0] - 2026-11-24

### Initial Release
- **Guided setup & file permissions**: Authorize specific folders or local fixed drives with background Everything filename indexing and on-demand rebuild.
- **Dedicated Muse token & file API**: Separate `/agent/v1` endpoints and `agent-token` lifecycle (issue, copy, locate, rotate, revoke) covering search, list, read, write, and mkdir.
- **Public tunnels, tray controls & Windows packages**: Tailscale Funnel by default (plus Cloudflare tunnels), tray controls, access logs, English / Simplified Chinese UI, and portable ZIP / NSIS packages.
