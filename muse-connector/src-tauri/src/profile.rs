//! 不可变运行 profile；开发入口不能用可编辑配置切换到生产资源。
use crate::core::{config::BridgeConfig, storage, token};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Development,
    Production,
}

#[derive(Debug, Clone)]
pub struct RuntimeProfile {
    pub kind: ProfileKind,
    pub home: PathBuf,
    pub onboarding_qa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    #[serde(alias = "cloudflare_quick")]
    Tailscale,
    CloudflareNamed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub tunnel_id: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Language {
    #[serde(rename = "zh-CN")]
    Chinese,
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ja")]
    Japanese,
}

impl Language {
    pub fn detect_system() -> Self {
        Self::from_locale_tags(system_locale_tags())
    }

    pub fn from_locale_tag(tag: &str) -> Option<Self> {
        let primary = normalize_locale_primary(tag)?;
        match primary.as_str() {
            "zh" | "zho" | "chi" => Some(Self::Chinese),
            "ja" | "jpn" => Some(Self::Japanese),
            "en" | "eng" | "c" | "posix" => Some(Self::English),
            _ => None,
        }
    }

    pub fn from_locale_tags<I, S>(tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for tag in tags {
            if let Some(language) = Self::from_locale_tag(tag.as_ref()) {
                return language;
            }
        }
        Self::English
    }

    pub fn text(
        self,
        chinese: &'static str,
        english: &'static str,
        japanese: &'static str,
    ) -> &'static str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
            Self::Japanese => japanese,
        }
    }

    pub fn product_name(self) -> &'static str {
        self.text("Muse连接器", "MuseLink", "MuseLink")
    }
}

fn normalize_locale_primary(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip POSIX encoding (`.UTF-8`) and modifier (`@euro` / `@latin`).
    let base = trimmed.split(['.', '@']).next().unwrap_or("").trim();
    if base.is_empty() {
        return None;
    }
    let primary = base.split(['-', '_']).next().unwrap_or("").trim();
    if primary.is_empty() {
        None
    } else {
        Some(primary.to_ascii_lowercase())
    }
}

#[cfg(any(unix, test))]
fn is_posix_c_locale(raw: &str) -> bool {
    let base = raw.trim().split(['.', '@']).next().unwrap_or("").trim();
    base.eq_ignore_ascii_case("c") || base.eq_ignore_ascii_case("posix")
}

#[cfg(any(unix, test))]
fn posix_locale_tags(
    lc_all: Option<&str>,
    lc_messages: Option<&str>,
    lang: Option<&str>,
    language_list: Option<&str>,
) -> Vec<String> {
    fn non_empty(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|s| !s.is_empty())
    }
    let primary = non_empty(lc_all)
        .or_else(|| non_empty(lc_messages))
        .or_else(|| non_empty(lang));
    if let Some(primary) = primary
        && is_posix_c_locale(primary)
    {
        // Per GNU gettext, `LANGUAGE` is ignored when the effective POSIX locale is `C` or `POSIX`.
        return vec![primary.to_owned()];
    }
    let mut tags = Vec::new();
    if let Some(list) = non_empty(language_list) {
        for item in list.split(':') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                tags.push(trimmed.to_owned());
            }
        }
    }
    if let Some(primary) = primary {
        tags.push(primary.to_owned());
    }
    tags
}

#[cfg(unix)]
fn posix_locale_tags_from_env() -> Vec<String> {
    let lc_all = std::env::var("LC_ALL").ok();
    let lc_messages = std::env::var("LC_MESSAGES").ok();
    let lang = std::env::var("LANG").ok();
    let language_list = std::env::var("LANGUAGE").ok();
    posix_locale_tags(
        lc_all.as_deref(),
        lc_messages.as_deref(),
        lang.as_deref(),
        language_list.as_deref(),
    )
}

#[cfg(windows)]
fn system_locale_tags() -> Vec<String> {
    use windows_sys::Win32::Globalization::{
        GetUserDefaultLocaleName, GetUserPreferredUILanguages, MUI_LANGUAGE_NAME,
    };

    const MAX_UI_LANG_BUFFER: u32 = 16 * 1024;
    let mut count = 0_u32;
    let mut len = 0_u32;
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut len,
        )
    } != 0
        && (2..=MAX_UI_LANG_BUFFER).contains(&len)
    {
        let mut buffer = vec![0_u16; len as usize];
        if unsafe {
            GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                buffer.as_mut_ptr(),
                &mut len,
            )
        } != 0
        {
            let used = (len as usize).min(buffer.len());
            let tags: Vec<String> = buffer[..used]
                .split(|&ch| ch == 0)
                .filter(|slice| !slice.is_empty())
                .map(String::from_utf16_lossy)
                .collect();
            if !tags.is_empty() {
                return tags;
            }
        }
    }

    // LOCALE_NAME_MAX_LENGTH is 85 UTF-16 code units including the trailing NUL.
    let mut locale = [0_u16; 85];
    let written = unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), locale.len() as i32) };
    if written > 1 {
        let end = ((written as usize) - 1).min(locale.len());
        let name = String::from_utf16_lossy(&locale[..end]);
        if !name.trim().is_empty() {
            return vec![name];
        }
    }
    Vec::new()
}

#[cfg(target_os = "macos")]
fn system_locale_tags() -> Vec<String> {
    let tags: Vec<String> = objc2_foundation::NSLocale::preferredLanguages()
        .to_vec()
        .into_iter()
        .map(|item| item.to_string())
        .filter(|item| !item.trim().is_empty())
        .collect();
    if !tags.is_empty() {
        return tags;
    }
    posix_locale_tags_from_env()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_locale_tags() -> Vec<String> {
    posix_locale_tags_from_env()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceNameUpdate {
    pub id: String,
    pub pending: bool,
}

fn default_true() -> bool {
    true
}

fn default_language() -> Language {
    Language::detect_system()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopState {
    pub schema_version: u32,
    pub desired_enabled: bool,
    pub provider: ProviderSettings,
    pub start_minimized: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default = "default_language")]
    pub language: Language,
    pub device_name: String,
    pub device_name_update: Option<DeviceNameUpdate>,
    pub exec_enabled: bool,
    pub legacy_api_enabled: bool,
    pub permission_mode: String,
    pub capabilities: Vec<String>,
    pub agent_token_ref: Option<String>,
}
impl Default for DesktopState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            desired_enabled: false,
            provider: ProviderSettings::default(),
            start_minimized: false,
            close_to_tray: true,
            language: default_language(),
            device_name: String::new(),
            device_name_update: None,
            exec_enabled: false,
            legacy_api_enabled: false,
            permission_mode: "request_approval".into(),
            capabilities: vec![],
            agent_token_ref: None,
        }
    }
}

pub(crate) fn valid_device_name(name: &str) -> bool {
    (1..=32).contains(&name.chars().count())
        && !name.starts_with(' ')
        && !name.ends_with(' ')
        && name
            .chars()
            .all(|c| c.is_alphabetic() || c.is_ascii_digit() || matches!(c, ' ' | '-' | '_'))
}
impl RuntimeProfile {
    pub fn discover() -> Result<Self, String> {
        let args: Vec<_> = std::env::args().collect();
        let requested = args
            .windows(2)
            .find(|p| p[0] == "--profile")
            .map(|p| p[1].as_str());
        if cfg!(debug_assertions) && requested == Some("production") {
            return Err("调试版本不能操作生产 profile".into());
        }
        let onboarding_qa = requested == Some("onboarding-qa");
        let development = cfg!(debug_assertions)
            || onboarding_qa
            || matches!(requested, Some("dev" | "development"));
        if requested
            .is_some_and(|p| !["dev", "development", "onboarding-qa", "production"].contains(&p))
        {
            return Err("未知运行 profile".into());
        }
        let home = if development {
            std::env::var_os("MUSE_CONNECTOR_DEV_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::data_local_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join("MuseConnector")
                        .join(if onboarding_qa {
                            "onboarding-qa"
                        } else {
                            "development"
                        })
                })
        } else {
            crate::core::config::bridge_home()
        };
        let profile = Self {
            kind: if development {
                ProfileKind::Development
            } else {
                ProfileKind::Production
            },
            home,
            onboarding_qa,
        };
        if profile.is_development()
            && same_location(&profile.home, &crate::core::config::bridge_home())
        {
            return Err("开发数据目录不能指向生产目录".into());
        }
        Ok(profile)
    }
    pub fn is_development(&self) -> bool {
        self.kind == ProfileKind::Development
    }
    pub fn window_title(&self, language: Language) -> String {
        let suffix = if self.onboarding_qa {
            language.text(" · 首次配置验收", " · Setup QA", " · 初回設定の確認")
        } else if self.is_development() {
            language.text(" · 开发隔离", " · Development", " · 開発モード")
        } else {
            ""
        };
        format!("{}{suffix}", language.product_name())
    }
    pub fn require_production(&self) -> Result<(), String> {
        if self.is_development() {
            Err("开发模式使用模拟适配器，禁止修改生产任务、隧道与自启项".into())
        } else {
            Ok(())
        }
    }
    pub fn validate_config_scope(&self, config: &BridgeConfig) -> Result<(), String> {
        if !self.is_development() {
            return Ok(());
        }
        let base = std::fs::canonicalize(&self.home).map_err(|_| "开发数据目录不可用")?;
        for root in config.roots.values() {
            let resolved = std::fs::canonicalize(root).map_err(|_| "开发测试目录不存在")?;
            if !resolved.starts_with(&base) {
                return Err("开发模式只允许开放独立开发数据目录内的测试文件夹".into());
            }
        }
        Ok(())
    }
    pub fn config_path(&self) -> PathBuf {
        self.home.join("config.json")
    }
    pub fn token_path(&self) -> PathBuf {
        self.home.join("token")
    }
    pub fn state_path(&self) -> PathBuf {
        self.home.join("desktop-state.json")
    }
    pub fn default_port(&self) -> u16 {
        if self.is_development() { 18795 } else { 18790 }
    }
    pub fn load_desktop(&self) -> Result<DesktopState, String> {
        self.load_desktop_with_meta().map(|(state, _)| state)
    }
    pub fn load_desktop_with_meta(&self) -> Result<(DesktopState, bool), String> {
        match std::fs::read_to_string(self.state_path()) {
            Ok(raw) => {
                let cleaned = raw.trim_start_matches('\u{feff}');
                let value: serde_json::Value = serde_json::from_str(cleaned)
                    .map_err(|_| "桌面状态损坏，请先修复或恢复配置".to_string())?;
                let legacy_api_explicit = value
                    .as_object()
                    .is_some_and(|map| map.contains_key("legacy_api_enabled"));
                let state: DesktopState = serde_json::from_value(value)
                    .map_err(|_| "桌面状态损坏，请先修复或恢复配置".to_string())?;
                if state.schema_version != 1 {
                    return Err("桌面状态版本不受支持".into());
                }
                // 当前只有结构预留，不能通过编辑 JSON 获得尚未实施的 Agent 权限。
                if !state.capabilities.is_empty()
                    || state.agent_token_ref.is_some()
                    || state.permission_mode != "request_approval"
                {
                    return Err("Agent 能力尚未启用，不能通过配置授予权限".into());
                }
                Ok((state, legacy_api_explicit))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok((DesktopState::default(), false))
            }
            Err(_) => Err("无法读取桌面状态".into()),
        }
    }
    pub fn save_desktop(&self, state: &DesktopState) -> Result<(), String> {
        let mut value = std::fs::read_to_string(self.state_path())
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        for (k, v) in serde_json::to_value(state)
            .map_err(|e| e.to_string())?
            .as_object()
            .unwrap()
        {
            value[k] = v.clone();
        }
        storage::atomic_write(
            &self.state_path(),
            &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|_| "保存桌面状态失败".into())
    }
    pub fn initialize_development(&self) -> Result<(), String> {
        if !self.is_development() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.home).map_err(|e| e.to_string())?;
        if self.onboarding_qa {
            let sample = self.home.join("files");
            std::fs::create_dir_all(&sample).map_err(|e| e.to_string())?;
            let canonical = std::fs::canonicalize(&sample).map_err(|e| e.to_string())?;
            let home = std::fs::canonicalize(&self.home).map_err(|e| e.to_string())?;
            if !canonical.starts_with(home) {
                return Err("验收目录必须位于独立开发数据目录内".into());
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(sample.join("hello.txt"))
            {
                Ok(mut file) => std::io::Write::write_all(&mut file, b"Hello, Muse!\n")
                    .map_err(|e| e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.to_string()),
            }
            return Ok(());
        }
        if !self.config_path().exists() {
            let root = self.home.join("files");
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let cfg = BridgeConfig {
                port: 18795,
                read_only: true,
                roots: [("development".into(), root)].into(),
            };
            cfg.save_to(&self.config_path())
                .map_err(|e| e.to_string())?;
        }
        if !self.token_path().exists() {
            token::save(&self.token_path(), &token::generate())
                .map_err(|_| "初始化开发令牌失败".to_string())?;
        }
        Ok(())
    }
}
fn same_location(a: &Path, b: &Path) -> bool {
    let normalize = |p: &Path| {
        crate::core::path_guard::canonical_non_strict(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_lowercase()
    };
    let a = normalize(a);
    let b = normalize(b);
    // Accept both Windows (`\`) and Unix (`/`) separators so nested-home checks work on Linux.
    a == b
        || a.starts_with(&(b.clone() + "\\"))
        || b.starts_with(&(a.clone() + "\\"))
        || a.starts_with(&(b.clone() + "/"))
        || b.starts_with(&(a + "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_language_defaults_for_existing_state_and_survives_reload() {
        let home = std::env::temp_dir().join(format!("muse-language-{}", token::generate()));
        std::fs::create_dir(&home).unwrap();
        let profile = RuntimeProfile {
            kind: ProfileKind::Development,
            home: home.clone(),
            onboarding_qa: false,
        };
        let detected = Language::detect_system();
        let fresh = profile.load_desktop().unwrap();
        assert_eq!(fresh.language, detected);
        assert!(
            !profile.state_path().exists(),
            "first-run locale detection must not pre-create desktop-state.json"
        );

        std::fs::write(profile.state_path(), r#"{"start_minimized":true}"#).unwrap();
        let mut state = profile.load_desktop().unwrap();
        assert_eq!(state.language, detected);
        state.language = Language::English;
        profile.save_desktop(&state).unwrap();

        let reopened = profile.load_desktop().unwrap();
        assert_eq!(reopened.language, Language::English);
        assert!(reopened.start_minimized);
        assert_eq!(
            profile.window_title(reopened.language),
            "MuseLink · Development"
        );
        state.language = Language::Chinese;
        profile.save_desktop(&state).unwrap();
        let reopened_zh = profile.load_desktop().unwrap();
        assert_eq!(reopened_zh.language, Language::Chinese);
        assert_eq!(
            profile.window_title(reopened_zh.language),
            "Muse连接器 · 开发隔离"
        );
        state.language = Language::Japanese;
        profile.save_desktop(&state).unwrap();
        let reopened_ja = profile.load_desktop().unwrap();
        assert_eq!(reopened_ja.language, Language::Japanese);
        assert_eq!(
            profile.window_title(reopened_ja.language),
            "MuseLink · 開発モード"
        );
        assert!(!DesktopState::default().start_minimized);
        assert!(DesktopState::default().close_to_tray);
        std::fs::write(
            profile.state_path(),
            r#"{"close_to_tray":false,"provider":{"kind":"cloudflare_quick"}}"#,
        )
        .unwrap();
        let legacy = profile.load_desktop().unwrap();
        assert!(!legacy.close_to_tray);
        assert_eq!(legacy.provider.kind, ProviderKind::Tailscale);
        std::fs::remove_file(profile.state_path()).unwrap();
        std::fs::remove_dir(home).unwrap();
    }

    #[test]
    fn locale_tags_map_supported_languages_and_fallback_to_english() {
        for tag in [
            "zh",
            "zh-CN",
            "zh_CN.UTF-8",
            "zh-Hans",
            "zh-Hans-CN",
            "zh-SG",
            "zh-TW",
            "zh_TW.UTF-8",
            "zh-HK",
            "zh-Hant-HK",
            "ZH_CN",
        ] {
            assert_eq!(
                Language::from_locale_tag(tag),
                Some(Language::Chinese),
                "expected Chinese for {tag}"
            );
        }
        for tag in ["ja", "ja-JP", "ja_JP.UTF-8", "JA_JP@euro"] {
            assert_eq!(
                Language::from_locale_tag(tag),
                Some(Language::Japanese),
                "expected Japanese for {tag}"
            );
        }
        for tag in ["en", "en-US", "en_GB.UTF-8", "C", "C.UTF-8", "POSIX"] {
            assert_eq!(
                Language::from_locale_tag(tag),
                Some(Language::English),
                "expected English for {tag}"
            );
        }
        for tag in ["", "  ", "fr-FR", "de_DE.UTF-8", "ko-KR", "es-ES", "ru_RU"] {
            assert_eq!(
                Language::from_locale_tag(tag),
                None,
                "expected None for unsupported {tag}"
            );
        }

        assert_eq!(
            Language::from_locale_tags(["fr-FR", "de-DE"]),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(["en-US", "zh-CN"]),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(["fr-FR", "en-GB", "zh-CN"]),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(["zh-TW", "en-US"]),
            Language::Chinese
        );
        assert_eq!(
            Language::from_locale_tags(["de-DE", "ja-JP"]),
            Language::Japanese
        );
        assert_eq!(
            Language::from_locale_tags([] as [&str; 0]),
            Language::English
        );
    }

    #[test]
    fn posix_locale_env_follows_priority_and_c_locale_fallback() {
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(None, None, None, None)),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(
                Some("C"),
                None,
                Some("zh_CN.UTF-8"),
                Some("zh_CN:ja")
            )),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(None, None, Some("POSIX"), Some("zh_CN"))),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(
                None,
                None,
                Some("C.UTF-8"),
                Some("ja_JP")
            )),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(
                None,
                Some("ja_JP.UTF-8"),
                Some("zh_CN.UTF-8"),
                None
            )),
            Language::Japanese
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(
                Some("fr_FR.UTF-8"),
                None,
                Some("zh_CN.UTF-8"),
                None
            )),
            Language::English
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(
                None,
                None,
                Some("en_US.UTF-8"),
                Some("fr:zh_TW:en")
            )),
            Language::Chinese
        );
        assert_eq!(
            Language::from_locale_tags(posix_locale_tags(None, None, Some("zh_CN.UTF-8"), None)),
            Language::Chinese
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_system_locale_query_returns_non_empty_tags() {
        let tags = system_locale_tags();
        assert!(
            !tags.is_empty(),
            "Windows UI locale query should return at least one tag"
        );
    }

    #[test]
    fn development_cannot_write_production_and_pause_survives_reload() {
        let home = std::env::temp_dir().join(format!("muse-profile-{}", token::generate()));
        let p = RuntimeProfile {
            kind: ProfileKind::Development,
            home: home.clone(),
            onboarding_qa: false,
        };
        assert!(p.require_production().is_err());
        p.initialize_development().unwrap();
        let mut cfg = BridgeConfig::load_from(&p.config_path()).unwrap();
        assert!(p.validate_config_scope(&cfg).is_ok());
        cfg.roots.insert("outside".into(), std::env::temp_dir());
        assert!(p.validate_config_scope(&cfg).is_err());
        assert!(same_location(&home, home.parent().unwrap()));
        assert!(same_location(home.parent().unwrap(), &home));
        let mut state = DesktopState {
            desired_enabled: true,
            ..Default::default()
        };
        p.save_desktop(&state).unwrap();
        state.desired_enabled = false;
        p.save_desktop(&state).unwrap();
        assert!(!p.load_desktop().unwrap().desired_enabled);
        assert_eq!(
            BridgeConfig::load_from(&p.config_path()).unwrap().port,
            18795
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
