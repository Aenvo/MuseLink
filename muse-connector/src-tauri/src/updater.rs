//! 应用内自动更新与同步更新日志模块。
//!
//! 安全与行为约束：
//! - 正式构建直接使用 `tauri.conf.json` 中编译期固化的 Minisign 公钥与 HTTPS 端点，
//!   不接受运行时环境变量覆盖，防止环境污染劫持更新来源。
//! - 仅 Windows NSIS 安装版支持应用内被动静默安装与自动重启（`/P /UPDATE /R`）；
//!   便携版 ZIP、Linux 与开发模式仅支持版本检查与更新日志同步，由用户按需前往 Releases 下载。
//! - 下载与签名校验期间保持现有连接不断流；校验通过后先停止隧道、本地 API、命令执行与
//!   托管 Everything 索引进程，再拉起安装器；若安装器拉起失败则自动尝试恢复服务。

use crate::profile::{Language, RuntimeProfile};
use crate::service::ServiceController;
use serde::Serialize;
use std::collections::HashMap;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Error as UpdaterError, Update, UpdaterExt};

pub const RELEASES_PAGE_URL: &str = "https://github.com/Aenvo/MuseLink/releases/latest";
const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/Aenvo/MuseLink/releases/latest/download/latest.json";
const CHANGELOG_RAW_ZH_URL: &str =
    "https://raw.githubusercontent.com/Aenvo/MuseLink/main/CHANGELOG.md";
const CHANGELOG_RAW_EN_URL: &str =
    "https://raw.githubusercontent.com/Aenvo/MuseLink/main/CHANGELOG.en.md";
const CHANGELOG_RAW_JA_URL: &str =
    "https://raw.githubusercontent.com/Aenvo/MuseLink/main/CHANGELOG.ja.md";
const CHANGELOG_WEB_ZH_URL: &str = "https://github.com/Aenvo/MuseLink/blob/main/CHANGELOG.md";
const CHANGELOG_WEB_EN_URL: &str = "https://github.com/Aenvo/MuseLink/blob/main/CHANGELOG.en.md";
const CHANGELOG_WEB_JA_URL: &str = "https://github.com/Aenvo/MuseLink/blob/main/CHANGELOG.ja.md";

const BUNDLED_CHANGELOG_ZH: &str = include_str!("../../../CHANGELOG.md");
const BUNDLED_CHANGELOG_EN: &str = include_str!("../../../CHANGELOG.en.md");
const BUNDLED_CHANGELOG_JA: &str = include_str!("../../../CHANGELOG.ja.md");

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const CHANGELOG_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_CHANGELOG_BYTES: usize = 512 * 1024;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const PROGRESS_EMIT_STEP_BYTES: u64 = 64 * 1024;
const PROGRESS_EVENT: &str = "update-progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InstallKind {
    Nsis,
    AppImage,
    Portable,
    Linux,
    Development,
    Unsupported,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nsis => "nsis",
            Self::AppImage => "appimage",
            Self::Portable => "portable",
            Self::Linux => "linux",
            Self::Development => "development",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn can_install_in_app(self) -> bool {
        matches!(self, Self::Nsis | Self::AppImage)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub configured: bool,
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
    pub can_install_in_app: bool,
    pub install_kind: String,
    pub releases_url: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgress {
    pub stage: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangelogResponse {
    pub markdown: String,
    pub source: String,
    pub synced_at: Option<String>,
    pub remote_url: String,
}

#[derive(Debug, Clone)]
struct CachedChangelog {
    markdown: String,
    synced_at: String,
}

#[derive(Default)]
pub struct UpdaterRuntime {
    pending: Mutex<Option<Update>>,
    changelogs: Mutex<HashMap<&'static str, CachedChangelog>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DebugOverride {
    endpoint: String,
    pubkey: Option<String>,
}

fn debug_install_qa_enabled() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("MUSE_CONNECTOR_UPDATE_QA").ok().as_deref() == Some("1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

pub fn install_kind(profile: &RuntimeProfile) -> InstallKind {
    if profile.is_development() {
        if cfg!(target_os = "windows") && debug_install_qa_enabled() {
            return InstallKind::Nsis;
        }
        return InstallKind::Development;
    }
    #[cfg(target_os = "windows")]
    {
        let installed = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(has_nsis_uninstaller))
            .unwrap_or(false);
        if installed {
            InstallKind::Nsis
        } else {
            InstallKind::Portable
        }
    }
    #[cfg(target_os = "linux")]
    {
        let running_appimage = std::env::var_os("APPIMAGE")
            .is_some_and(|path| !path.is_empty() && Path::new(&path).is_file());
        if running_appimage {
            InstallKind::AppImage
        } else {
            InstallKind::Linux
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        InstallKind::Unsupported
    }
}

#[cfg(any(target_os = "windows", test))]
fn has_nsis_uninstaller(install_dir: &Path) -> bool {
    install_dir.join("uninstall.exe").is_file()
}

pub fn is_configured() -> bool {
    true
}

#[cfg(any(debug_assertions, test))]
fn is_loopback_http(parsed: &reqwest::Url) -> bool {
    parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

#[cfg(any(debug_assertions, test))]
fn parse_debug_override(endpoint: String, key: Option<String>) -> Option<DebugOverride> {
    let pubkey = match key {
        Some(value) if value.trim().is_empty() => return None,
        Some(value) => Some(value),
        None => None,
    };
    let parsed = reqwest::Url::parse(&endpoint).ok()?;
    let scheme_ok = parsed.scheme() == "https" || is_loopback_http(&parsed);
    if !scheme_ok || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    Some(DebugOverride { endpoint, pubkey })
}

fn debug_override() -> Option<DebugOverride> {
    #[cfg(debug_assertions)]
    {
        let endpoint = std::env::var("MUSE_CONNECTOR_UPDATE_ENDPOINT").ok()?;
        let key = std::env::var("MUSE_CONNECTOR_UPDATE_PUBLIC_KEY").ok();
        parse_debug_override(endpoint, key)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder().timeout(CHECK_TIMEOUT);
    if let Some(override_cfg) = debug_override() {
        let parsed = override_cfg
            .endpoint
            .parse()
            .map_err(|_| "更新源地址无效".to_string())?;
        if let Some(pubkey) = override_cfg.pubkey {
            builder = builder.pubkey(pubkey);
        }
        builder = builder
            .endpoints(vec![parsed])
            .map_err(|_| "更新源必须使用 HTTPS".to_string())?;
    }
    builder
        .build()
        .map_err(|_| "更新配置无效，请稍后重试".to_string())
}

fn check_error_message(error: &UpdaterError) -> String {
    match error {
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => {
            "更新检查失败，请检查网络连接后重试".into()
        }
        _ => "更新检查暂时不可用，请稍后重试".into(),
    }
}

fn install_error_message(error: &UpdaterError) -> String {
    match error {
        UpdaterError::Minisign(_) => "更新包签名校验失败，已停止安装；当前连接保持原状态".into(),
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => {
            "更新下载失败，请检查网络后重试；当前连接保持原状态".into()
        }
        _ => "更新下载或校验失败，当前连接保持原状态".into(),
    }
}

fn changelog_urls(language: Language) -> (&'static str, &'static str, &'static str, &'static str) {
    match language {
        Language::Chinese => (
            "zh-CN",
            BUNDLED_CHANGELOG_ZH,
            CHANGELOG_RAW_ZH_URL,
            CHANGELOG_WEB_ZH_URL,
        ),
        Language::English => (
            "en",
            BUNDLED_CHANGELOG_EN,
            CHANGELOG_RAW_EN_URL,
            CHANGELOG_WEB_EN_URL,
        ),
        Language::Japanese => (
            "ja",
            BUNDLED_CHANGELOG_JA,
            CHANGELOG_RAW_JA_URL,
            CHANGELOG_WEB_JA_URL,
        ),
    }
}

#[tauri::command]
pub fn get_bundled_changelog(
    language: Language,
    runtime: State<'_, UpdaterRuntime>,
) -> ChangelogResponse {
    let (key, bundled, _, web_url) = changelog_urls(language);
    if let Ok(cache) = runtime.changelogs.lock()
        && let Some(cached) = cache.get(key)
    {
        return ChangelogResponse {
            markdown: cached.markdown.clone(),
            source: "remote".into(),
            synced_at: Some(cached.synced_at.clone()),
            remote_url: web_url.into(),
        };
    }
    ChangelogResponse {
        markdown: bundled.to_owned(),
        source: "bundled".into(),
        synced_at: None,
        remote_url: web_url.into(),
    }
}

#[tauri::command]
pub async fn fetch_remote_changelog(
    language: Language,
    runtime: State<'_, UpdaterRuntime>,
) -> Result<ChangelogResponse, String> {
    let (key, _, raw_url, web_url) = changelog_urls(language);
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(CHANGELOG_TIMEOUT)
        .user_agent(concat!("MuseLink/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法初始化更新日志请求客户端".to_string())?;
    let mut response = client
        .get(raw_url)
        .send()
        .await
        .map_err(|_| "无法连接 GitHub 同步更新日志".to_string())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("暂时无法获取最新更新日志".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "更新日志读取中断".to_string())?
    {
        if bytes.len() + chunk.len() > MAX_CHANGELOG_BYTES {
            return Err("更新日志内容超出允许大小".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let markdown = String::from_utf8(bytes).map_err(|_| "更新日志编码无效".to_string())?;
    if !markdown.contains("## [") {
        return Err("更新日志格式异常".into());
    }
    let synced_at = chrono::Utc::now().to_rfc3339();
    if let Ok(mut cache) = runtime.changelogs.lock() {
        cache.insert(
            key,
            CachedChangelog {
                markdown: markdown.clone(),
                synced_at: synced_at.clone(),
            },
        );
    }
    Ok(ChangelogResponse {
        markdown,
        source: "remote".into(),
        synced_at: Some(synced_at),
        remote_url: web_url.into(),
    })
}

#[derive(Debug, serde::Deserialize)]
struct StaticReleaseManifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
}

fn parse_semver_triple(version: &str) -> Option<(u64, u64, u64, bool)> {
    let trimmed = version.trim().trim_start_matches(['v', 'V']);
    let (core, is_prerelease) = match trimmed.split_once('-') {
        Some((c, _)) => (c, true),
        None => (trimmed, false),
    };
    let core = core.split_once('+').map_or(core, |(c, _)| c);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, is_prerelease))
}

fn is_newer_version(current: &str, remote: &str) -> bool {
    let (Some((cmaj, cmin, cpat, cpre)), Some((rmaj, rmin, rpat, rpre))) =
        (parse_semver_triple(current), parse_semver_triple(remote))
    else {
        return false;
    };
    match (rmaj, rmin, rpat).cmp(&(cmaj, cmin, cpat)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // 同主次修订号时，正式版高于预发布版
        std::cmp::Ordering::Equal => cpre && !rpre,
    }
}

async fn check_static_manifest(
    current_version: String,
    kind: InstallKind,
    state: &Arc<ServiceController>,
) -> Result<UpdateCheckResult, String> {
    let endpoint = debug_override()
        .map(|cfg| cfg.endpoint)
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_owned());
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            let url = attempt.url();
            let allowed = url.scheme() == "https"
                || cfg!(debug_assertions)
                    && url.scheme() == "http"
                    && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
            if allowed {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .timeout(CHECK_TIMEOUT)
        .user_agent(concat!("MuseLink/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "更新检查暂时不可用，请稍后重试".to_string())?;
    let mut response = client.get(&endpoint).send().await.map_err(|error| {
        state
            .logs
            .runtime_entry("warn", &format!("更新检查失败：{error}"));
        "更新检查失败，请检查网络连接后重试".to_string()
    })?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        state
            .logs
            .runtime_entry("info", "更新源尚无可用更新清单，按已是最新版本处理");
        return Ok(UpdateCheckResult {
            configured: true,
            available: false,
            current_version,
            version: None,
            notes: None,
            date: None,
            can_install_in_app: false,
            install_kind: kind.as_str().into(),
            releases_url: RELEASES_PAGE_URL.into(),
            message: Some("当前已是最新版本".into()),
        });
    }
    if response.status() != reqwest::StatusCode::OK {
        return Err("更新检查暂时不可用，请稍后重试".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "更新检查失败，请检查网络连接后重试".to_string())?
    {
        if bytes.len() + chunk.len() > MAX_MANIFEST_BYTES {
            return Err("更新检查暂时不可用，请稍后重试".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let manifest: StaticReleaseManifest =
        serde_json::from_slice(&bytes).map_err(|_| "更新检查暂时不可用，请稍后重试".to_string())?;
    let remote_version = manifest
        .version
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_owned();
    if is_newer_version(&current_version, &remote_version) {
        state
            .logs
            .runtime_entry("info", &format!("发现新版本 v{remote_version}"));
        Ok(UpdateCheckResult {
            configured: true,
            available: true,
            current_version,
            version: Some(remote_version),
            notes: manifest.notes,
            date: manifest.pub_date,
            can_install_in_app: false,
            install_kind: kind.as_str().into(),
            releases_url: RELEASES_PAGE_URL.into(),
            message: None,
        })
    } else {
        Ok(UpdateCheckResult {
            configured: true,
            available: false,
            current_version,
            version: None,
            notes: None,
            date: None,
            can_install_in_app: false,
            install_kind: kind.as_str().into(),
            releases_url: RELEASES_PAGE_URL.into(),
            message: Some("当前已是最新版本".into()),
        })
    }
}

#[tauri::command]
pub async fn check_updates(
    app: AppHandle,
    state: State<'_, Arc<ServiceController>>,
    runtime: State<'_, UpdaterRuntime>,
) -> Result<UpdateCheckResult, String> {
    let kind = install_kind(&state.profile);
    let current_version = app.package_info().version.to_string();
    // 便携版、Linux .deb/源码构建等形态不需要匹配特定平台安装器，直接读取同一份静态
    // latest.json 比对语义化版本，确保新版本提醒与更新详情不漏报。
    if !kind.can_install_in_app() {
        if let Ok(mut pending) = runtime.pending.lock() {
            *pending = None;
        }
        return check_static_manifest(current_version, kind, state.inner()).await;
    }
    let updater = build_updater(&app)?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone();
            let date = update.date.map(|d| d.to_string());
            if let Ok(mut pending) = runtime.pending.lock() {
                *pending = Some(update);
            }
            state
                .logs
                .runtime_entry("info", &format!("发现新版本 v{version}"));
            Ok(UpdateCheckResult {
                configured: true,
                available: true,
                current_version,
                version: Some(version),
                notes,
                date,
                can_install_in_app: kind.can_install_in_app(),
                install_kind: kind.as_str().into(),
                releases_url: RELEASES_PAGE_URL.into(),
                message: None,
            })
        }
        Ok(None) => {
            if let Ok(mut pending) = runtime.pending.lock() {
                *pending = None;
            }
            Ok(UpdateCheckResult {
                configured: true,
                available: false,
                current_version,
                version: None,
                notes: None,
                date: None,
                can_install_in_app: kind.can_install_in_app(),
                install_kind: kind.as_str().into(),
                releases_url: RELEASES_PAGE_URL.into(),
                message: Some("当前已是最新版本".into()),
            })
        }
        Err(UpdaterError::ReleaseNotFound) => {
            if let Ok(mut pending) = runtime.pending.lock() {
                *pending = None;
            }
            state
                .logs
                .runtime_entry("info", "更新源尚无可用更新清单，按已是最新版本处理");
            Ok(UpdateCheckResult {
                configured: true,
                available: false,
                current_version,
                version: None,
                notes: None,
                date: None,
                can_install_in_app: kind.can_install_in_app(),
                install_kind: kind.as_str().into(),
                releases_url: RELEASES_PAGE_URL.into(),
                message: Some("当前已是最新版本".into()),
            })
        }
        Err(error) => {
            state
                .logs
                .runtime_entry("warn", &format!("更新检查失败：{error}"));
            Err(check_error_message(&error))
        }
    }
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, Arc<ServiceController>>,
    runtime: State<'_, UpdaterRuntime>,
) -> Result<(), String> {
    if !debug_install_qa_enabled() {
        state.profile.require_production()?;
    }
    let kind = install_kind(&state.profile);
    if !kind.can_install_in_app() {
        return Err("当前版本不支持应用内覆盖安装，请前往 GitHub Releases 下载新版".into());
    }
    let mut update = match runtime.pending.lock().ok().and_then(|mut slot| slot.take()) {
        Some(cached) => cached,
        None => {
            let updater = build_updater(&app)?;
            updater
                .check()
                .await
                .map_err(|error| check_error_message(&error))?
                .ok_or_else(|| "当前已是最新版本".to_string())?
        }
    };
    let target_version = update.version.clone();
    update.timeout = Some(DOWNLOAD_TIMEOUT);

    state
        .logs
        .runtime_entry("info", &format!("开始下载更新 v{target_version}"));
    let _ = app.emit(
        PROGRESS_EVENT,
        UpdateProgress {
            stage: "downloading".into(),
            downloaded: 0,
            total: None,
        },
    );

    let downloaded = Arc::new(AtomicU64::new(0));
    let last_emitted = Arc::new(AtomicU64::new(0));
    let last_total = Arc::new(Mutex::new(None::<u64>));
    let last_tick = Arc::new(Mutex::new(Instant::now()));

    let chunk_app = app.clone();
    let chunk_downloaded = Arc::clone(&downloaded);
    let chunk_last_emitted = Arc::clone(&last_emitted);
    let chunk_last_total = Arc::clone(&last_total);
    let chunk_last_tick = Arc::clone(&last_tick);

    let finish_app = app.clone();
    let finish_downloaded = Arc::clone(&downloaded);
    let finish_total = Arc::clone(&last_total);

    let bytes = match update
        .download(
            move |chunk_length, content_length| {
                let current = chunk_downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed)
                    + chunk_length as u64;
                if let Ok(mut total_slot) = chunk_last_total.lock() {
                    *total_slot = content_length;
                }
                let prev = chunk_last_emitted.load(Ordering::Relaxed);
                let elapsed_ok = chunk_last_tick
                    .lock()
                    .map(|t| t.elapsed() >= Duration::from_millis(120))
                    .unwrap_or(false);
                if current.saturating_sub(prev) >= PROGRESS_EMIT_STEP_BYTES
                    || elapsed_ok
                    || content_length.is_some_and(|total| current >= total)
                {
                    chunk_last_emitted.store(current, Ordering::Relaxed);
                    if let Ok(mut tick) = chunk_last_tick.lock() {
                        *tick = Instant::now();
                    }
                    let _ = chunk_app.emit(
                        PROGRESS_EVENT,
                        UpdateProgress {
                            stage: "downloading".into(),
                            downloaded: current,
                            total: content_length,
                        },
                    );
                }
            },
            move || {
                let current = finish_downloaded.load(Ordering::Relaxed);
                let total = finish_total.lock().ok().and_then(|v| *v).or(Some(current));
                let _ = finish_app.emit(
                    PROGRESS_EVENT,
                    UpdateProgress {
                        stage: "verifying".into(),
                        downloaded: current,
                        total,
                    },
                );
            },
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            state
                .logs
                .runtime_entry("warn", &format!("更新下载或校验失败：{error}"));
            return Err(install_error_message(&error));
        }
    };

    let final_bytes = downloaded.load(Ordering::Relaxed).max(bytes.len() as u64);
    let _ = app.emit(
        PROGRESS_EVENT,
        UpdateProgress {
            stage: "installing".into(),
            downloaded: final_bytes,
            total: Some(final_bytes),
        },
    );

    if let Err(error) = state.suspend_for_update().await {
        state
            .logs
            .runtime_entry("warn", &format!("更新前停服失败：{error}"));
        return Err("更新前停止本地服务失败，请稍后重试".into());
    }

    // Windows 上 update.install 会拉起被动模式 NSIS（/P /UPDATE /R）并退出主进程；
    // Linux AppImage 上 update.install 替换文件后会返回 Ok(())，需显式重启进入新版。
    // 提前置位 exiting 避免 ExitRequested 再次进入 20 秒异步关停流程。
    state.exiting.store(true, Ordering::SeqCst);
    match update.install(bytes) {
        Ok(()) => {
            #[cfg(not(target_os = "windows"))]
            {
                app.restart();
            }
            #[cfg(target_os = "windows")]
            {
                Ok(())
            }
        }
        Err(error) => {
            state.exiting.store(false, Ordering::SeqCst);
            state
                .logs
                .runtime_entry("warn", &format!("更新安装器启动失败：{error}"));
            state.resume_after_update_failure().await;
            Err("更新安装失败，已尝试恢复原连接；也可前往 GitHub Releases 手动更新".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn debug_override_and_release_metadata_fail_closed() {
        assert!(
            parse_debug_override("http://example.com/update".into(), Some("key".into())).is_none()
        );
        assert!(parse_debug_override("http://127.0.0.1:18899/latest.json".into(), None).is_some());
        assert!(
            parse_debug_override(
                "https://user:secret@example.com/update".into(),
                Some("key".into())
            )
            .is_none()
        );
        assert!(
            parse_debug_override("https://example.com/update".into(), Some(" ".into())).is_none()
        );
        assert!(
            parse_debug_override("https://example.com/update".into(), Some("key".into())).is_some()
        );

        let valid = json!({
            "version": "v0.2.2",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://github.com/Aenvo/MuseLink/releases/download/v0.2.2/MuseLink-0.2.2-windows-x64-setup.exe",
                    "signature": "disposable-invalid-signature"
                }
            }
        });
        let parsed: tauri_plugin_updater::RemoteRelease = serde_json::from_value(valid).unwrap();
        assert_eq!(parsed.version.to_string(), "0.2.2");
        assert!(parsed.download_url("windows-aarch64").is_err());
        assert!(
            serde_json::from_value::<tauri_plugin_updater::RemoteRelease>(
                json!({"version": "invalid"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<tauri_plugin_updater::RemoteRelease>(
                json!({"version": "0.2.2", "url": "https://example.com/update.exe"})
            )
            .is_err()
        );
    }

    #[test]
    fn bundled_changelogs_are_embedded_and_non_empty() {
        assert!(BUNDLED_CHANGELOG_ZH.contains("## [v0.2."));
        assert!(BUNDLED_CHANGELOG_EN.contains("## [v0.2."));
        assert!(BUNDLED_CHANGELOG_JA.contains("## [v0.2."));
        assert_eq!(
            changelog_urls(Language::Japanese),
            (
                "ja",
                BUNDLED_CHANGELOG_JA,
                CHANGELOG_RAW_JA_URL,
                CHANGELOG_WEB_JA_URL,
            )
        );
    }

    #[test]
    fn nsis_uninstaller_detection_distinguishes_installed_and_portable() {
        let dir = std::env::temp_dir().join(format!(
            "muselink-updater-kind-{}",
            crate::core::token::generate()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!has_nsis_uninstaller(&dir));
        std::fs::write(dir.join("uninstall.exe"), b"stub").unwrap();
        assert!(has_nsis_uninstaller(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn semver_comparison_handles_v_prefix_and_prereleases() {
        assert!(is_newer_version("0.2.2", "0.2.3"));
        assert!(is_newer_version("0.2.2", "v0.3.0"));
        assert!(is_newer_version("0.2.3-rc.1", "0.2.3"));
        assert!(!is_newer_version("0.2.3", "0.2.3"));
        assert!(!is_newer_version("0.2.3", "0.2.2"));
        assert!(!is_newer_version("0.2.3", "0.2.3-rc.1"));
        assert!(!is_newer_version("0.2.3", "invalid"));
    }
}
