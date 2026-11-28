//! 隧道适配器。开发 profile 不调用真实工具；只撤销本应用拥有的入口。
use crate::profile::{ProviderKind, ProviderSettings, RuntimeProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Health {
    Up,
    Degraded,
    Down,
    Paused,
    Unknown,
    Starting,
    Stopping,
    Conflict,
}
#[derive(Debug, Clone, Serialize)]
pub struct TunnelStatus {
    pub health: Health,
    pub api_up: bool,
    pub funnel_up: bool,
    pub url: Option<String>,
    pub enabled: bool,
    pub last_check: String,
    pub note: String,
    pub api_port: u16,
    pub public_verified: Option<bool>,
    pub agent_public_verified: Option<bool>,
}
impl Default for TunnelStatus {
    fn default() -> Self {
        Self {
            health: Health::Unknown,
            api_up: false,
            funnel_up: false,
            url: None,
            enabled: false,
            last_check: String::new(),
            note: "尚未探测".into(),
            api_port: 0,
            public_verified: None,
            agent_public_verified: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct TunnelObservation {
    pub configured: bool,
    pub online: bool,
    pub url: Option<String>,
    pub note: String,
}

impl TunnelObservation {
    pub(crate) fn simulated(owned: bool) -> Self {
        Self {
            configured: owned,
            online: owned,
            url: None,
            note: "开发模拟连接，不创建公网入口".into(),
        }
    }
}

/// Platform-specific CLI file name: Windows binaries carry `.exe`, Unix ones do not.
fn exe_file_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_owned()
    }
}

/// Normalize a caller-supplied executable name for the current platform.
/// Call sites may pass Windows-style `*.exe` names; strip the suffix on Unix.
fn platform_exe_name(name: &str) -> &str {
    if cfg!(windows) {
        name
    } else {
        name.strip_suffix(".exe").unwrap_or(name)
    }
}

/// CLI shipped inside the macOS Tailscale app bundle.
#[cfg(target_os = "macos")]
fn tailscale_app_bundle_exe() -> PathBuf {
    PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale")
}

pub fn discover_executable(name: &str) -> Option<PathBuf> {
    let name = platform_exe_name(name);
    if let Some(found) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    }) {
        return Some(found);
    }
    let mut candidates = vec![];
    if name == "tailscale.exe" {
        candidates.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
    }
    if let Some(local) = dirs::data_local_dir() {
        candidates.push(local.join("Microsoft/WinGet/Links").join(name));
    }
    if let Some(program) = std::env::var_os("ProgramFiles") {
        let program = PathBuf::from(program);
        candidates.push(program.join("cloudflared").join(name));
        candidates.push(program.join("WinGet/Links").join(name));
        if name == "cloudflared.exe" {
            for base in [
                program.join("WinGet/Packages"),
                dirs::data_local_dir()
                    .unwrap_or_default()
                    .join("Microsoft/WinGet/Packages"),
            ] {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten().filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("Cloudflare.cloudflared_")
                    }) {
                        candidates.push(entry.path().join(name));
                    }
                }
            }
        }
    }
    if cfg!(unix) {
        candidates.push(PathBuf::from("/usr/local/bin").join(name));
        // Apple Silicon Homebrew lives here; GUI apps start with a minimal
        // PATH that does not include it, so PATH lookup alone misses brew
        // installs of tailscale / cloudflared.
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
        candidates.push(PathBuf::from("/usr/bin").join(name));
    }
    // The macOS Tailscale app keeps its CLI inside the bundle under a
    // capitalised file name; treat it like the Windows Program Files
    // special case above.
    #[cfg(target_os = "macos")]
    if name == "tailscale" {
        candidates.push(tailscale_app_bundle_exe());
    }
    candidates.into_iter().find(|p| p.is_file())
}
pub fn command(exe: &std::path::Path) -> Command {
    let mut native = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        native.creation_flags(0x08000000);
    }
    let mut cmd = Command::from(native);
    cmd.kill_on_drop(true);
    cmd
}
pub async fn run_cli(exe: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = command(exe);
    cmd.args(args);
    let out = tokio::time::timeout(Duration::from_secs(12), cmd.output())
        .await
        .map_err(|_| "命令超时，子进程已取消".to_string())?
        .map_err(|e| format!("工具启动失败：{}", e.kind()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("请检查工具登录与权限");
        return Err(format!(
            "工具退出码 {:?}：{}",
            out.status.code(),
            first.chars().take(300).collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Owner {
    port: u16,
    provider: ProviderKind,
    #[serde(default = "crate::profile::default_tailscale_https_port")]
    https_port: u16,
}

pub struct TunnelManager {
    profile: RuntimeProfile,
    child: Option<Child>,
    #[cfg(windows)]
    child_job: Option<crate::core::job::ChildJob>,
    owned: bool,
    port: u16,
    current: ProviderSettings,
}
impl TunnelManager {
    pub fn new(profile: RuntimeProfile) -> Self {
        Self {
            profile,
            child: None,
            #[cfg(windows)]
            child_job: None,
            owned: false,
            port: 0,
            current: ProviderSettings::default(),
        }
    }
    fn owner_path(&self) -> PathBuf {
        self.profile.home.join("tunnel-owner.json")
    }
    fn remember(&self, settings: &ProviderSettings, port: u16) -> Result<(), String> {
        crate::core::storage::atomic_write(
            &self.owner_path(),
            &serde_json::to_vec(&Owner {
                port,
                provider: settings.kind.clone(),
                https_port: settings.effective_https_port(),
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|_| "不能保存隧道归属，已拒绝修改入口".into())
    }
    pub async fn start(
        &mut self,
        settings: &ProviderSettings,
        port: u16,
    ) -> Result<TunnelObservation, String> {
        if self.profile.is_development() {
            self.current = settings.clone();
            self.port = port;
            self.owned = true;
            return self.observe().await;
        }
        validate_provider(settings)?;
        match settings.kind {
            ProviderKind::Tailscale => {
                let https_port = settings.effective_https_port();
                let https_flag = format!("--https={https_port}");
                let exe = discover_executable(&exe_file_name("tailscale"))
                    .ok_or("未检测到 Tailscale，请先安装并登录")?;
                let raw = run_cli(&exe, &["funnel", "status", "--json"]).await?;
                let cfg: Value = serde_json::from_str(&raw)
                    .map_err(|_| "无法解析 Tailscale 配置，未改变入口")?;
                ensure_exclusive_mapping(&cfg, port, https_port)?;
                if has_serve_only_mapping(&cfg, port, https_port) {
                    return Err(format!(
                        "检测到 HTTPS 端口 {https_port} 已配置 Tailscale Serve（仅内网），已拒绝自动开启公网 Funnel；如需仅 Tailnet 内网访问，请在连接页切换为 Tailscale Serve"
                    ));
                }
                self.remember(settings, port)?;
                // 先记归属再执行：超时/崩溃后可以识别本次未完成操作。
                self.current = settings.clone();
                self.port = port;
                self.owned = true;
                run_cli(&exe, &["funnel", "--bg", &https_flag, &port.to_string()]).await?;
            }
            ProviderKind::TailscaleServe => {
                let https_port = settings.effective_https_port();
                let https_flag = format!("--https={https_port}");
                let exe = discover_executable(&exe_file_name("tailscale"))
                    .ok_or("未检测到 Tailscale，请先安装并登录")?;
                let raw = run_cli(&exe, &["serve", "status", "--json"]).await?;
                let cfg: Value = serde_json::from_str(&raw)
                    .map_err(|_| "无法解析 Tailscale 配置，未改变入口")?;
                ensure_exclusive_mapping(&cfg, port, https_port)?;
                if has_funnel_enabled_on_port(&cfg, https_port) {
                    return Err(format!(
                        "HTTPS 端口 {https_port} 当前已开启公网 Tailscale Funnel，已拒绝按仅内网模式接管；请先关闭 Funnel 或切换至 Tailscale Funnel 模式"
                    ));
                }
                self.remember(settings, port)?;
                // 先记归属再执行：超时/崩溃后可以识别本次未完成操作。
                self.current = settings.clone();
                self.port = port;
                self.owned = true;
                if !has_serve_only_mapping(&cfg, port, https_port) {
                    run_cli(&exe, &["serve", "--bg", &https_flag, &port.to_string()]).await?;
                }
            }
            ProviderKind::CloudflareNamed => {
                let exe = discover_executable(&exe_file_name("cloudflared"))
                    .ok_or("未检测到 cloudflared，请先安装")?;
                let mut cmd = command(&exe);
                cmd.arg("tunnel").arg("--no-autoupdate").args([
                    "--url",
                    &format!("http://127.0.0.1:{port}"),
                    "run",
                    &settings.tunnel_id,
                ]);
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                let child = cmd
                    .spawn()
                    .map_err(|e| format!("cloudflared 启动失败：{}", e.kind()))?;
                #[cfg(windows)]
                {
                    self.child_job = Some(
                        crate::core::job::ChildJob::attach_tokio(&child)
                            .map_err(|_| "无法建立隧道子进程退出保护，已取消启动")?,
                    );
                }
                self.child = Some(child);
                self.current = settings.clone();
                self.port = port;
                self.owned = true;
            }
        }
        self.observe().await
    }
    pub async fn stop(&mut self) -> Result<(), String> {
        if self.profile.is_development() {
            self.owned = false;
            return Ok(());
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            if tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .is_err()
            {
                self.child = Some(child);
                return Err("隧道子进程停止超时，文件服务将单独停止".into());
            }
        }
        #[cfg(windows)]
        {
            self.child_job.take();
        }
        if self.owned && self.current.kind.is_tailscale() {
            let https_port = self.current.effective_https_port();
            let https_flag = format!("--https={https_port}");
            let exe = discover_executable(&exe_file_name("tailscale"))
                .ok_or("Tailscale 工具不可用，无法清理入口")?;
            match self.current.kind {
                ProviderKind::Tailscale => {
                    let raw = run_cli(&exe, &["funnel", "status", "--json"]).await?;
                    let cfg: Value =
                        serde_json::from_str(&raw).map_err(|_| "无法确认隧道归属，未撤销入口")?;
                    ensure_exclusive_mapping(&cfg, self.port, https_port)?;
                    if has_mapping(&cfg, self.port, https_port) {
                        run_cli(&exe, &["funnel", &https_flag, "off"]).await?;
                    }
                }
                ProviderKind::TailscaleServe => {
                    let raw = run_cli(&exe, &["serve", "status", "--json"]).await?;
                    let cfg: Value =
                        serde_json::from_str(&raw).map_err(|_| "无法确认隧道归属，未撤销入口")?;
                    ensure_exclusive_mapping(&cfg, self.port, https_port)?;
                    if has_serve_only_mapping(&cfg, self.port, https_port)
                        && !has_funnel_enabled_on_port(&cfg, https_port)
                    {
                        run_cli(&exe, &["serve", &https_flag, "off"]).await?;
                    }
                }
                ProviderKind::CloudflareNamed => {}
            }
        }
        let was_owned = self.owned;
        self.owned = false;
        if was_owned && self.owner_path().exists() {
            std::fs::remove_file(self.owner_path())
                .map_err(|_| "入口已停止，但归属记录清理失败")?;
        }
        Ok(())
    }
    pub async fn observe(&mut self) -> Result<TunnelObservation, String> {
        if self.profile.is_development() {
            return Ok(TunnelObservation::simulated(self.owned));
        }
        let https_port = self.current.effective_https_port();
        match self.current.kind {
            ProviderKind::Tailscale => observe_tailscale(self.port, https_port).await,
            ProviderKind::TailscaleServe => observe_tailscale_serve(self.port, https_port).await,
            ProviderKind::CloudflareNamed => {
                let running = match self.child.as_mut() {
                    Some(child) => child
                        .try_wait()
                        .map_err(|_| "无法检查 cloudflared 进程")?
                        .is_none(),
                    None => false,
                };
                let url = Some(format!("https://{}", self.current.hostname));
                Ok(TunnelObservation {
                    configured: running,
                    online: running,
                    url,
                    note: if running {
                        "隧道进程运行中；公网来源尚未验证".into()
                    } else {
                        "隧道进程已退出".into()
                    },
                })
            }
        }
    }
    pub fn adopt_pending_owner(&mut self, settings: &ProviderSettings, port: u16) {
        self.port = port;
        self.current = settings.clone();
        if let Ok(raw) = std::fs::read(self.owner_path())
            && let Ok(owner) = serde_json::from_slice::<Owner>(&raw)
            && owner.provider == settings.kind
            && owner.port == port
            && owner.https_port == settings.effective_https_port()
        {
            self.owned = true;
            self.port = port;
            self.current = settings.clone();
        }
    }
    pub fn is_owned(&self) -> bool {
        self.owned
    }
    pub(crate) fn live_tailscale_target(&self, port: u16) -> Option<(ProviderKind, u16)> {
        (self.owned && self.current.kind.is_tailscale() && self.port == port).then(|| {
            (
                self.current.kind.clone(),
                self.current.effective_https_port(),
            )
        })
    }
}

pub(crate) async fn observe_tailscale_target(
    port: u16,
    kind: &ProviderKind,
    https_port: u16,
) -> Result<TunnelObservation, String> {
    match kind {
        ProviderKind::TailscaleServe => observe_tailscale_serve(port, https_port).await,
        _ => observe_tailscale(port, https_port).await,
    }
}

pub(crate) async fn observe_tailscale(
    port: u16,
    https_port: u16,
) -> Result<TunnelObservation, String> {
    observe_tailscale_mode(port, https_port, true).await
}

pub(crate) async fn observe_tailscale_serve(
    port: u16,
    https_port: u16,
) -> Result<TunnelObservation, String> {
    observe_tailscale_mode(port, https_port, false).await
}

async fn observe_tailscale_mode(
    port: u16,
    https_port: u16,
    require_funnel: bool,
) -> Result<TunnelObservation, String> {
    let exe = discover_executable(&exe_file_name("tailscale")).ok_or("未安装 Tailscale")?;
    let subcommand = if require_funnel { "funnel" } else { "serve" };
    let raw = run_cli(&exe, &[subcommand, "status", "--json"]).await?;
    let cfg: Value = serde_json::from_str(&raw).map_err(|_| "Tailscale 状态未知：配置解析失败")?;
    let node: Value = serde_json::from_str(&run_cli(&exe, &["status", "--json"]).await?)
        .map_err(|_| "节点状态解析失败")?;
    let online = node["BackendState"] == "Running" && node["Self"]["Online"] == true;
    Ok(evaluate_tailscale_status(
        &cfg,
        online,
        port,
        https_port,
        require_funnel,
    ))
}

fn evaluate_tailscale_status(
    cfg: &Value,
    online: bool,
    port: u16,
    https_port: u16,
    require_funnel: bool,
) -> TunnelObservation {
    if require_funnel {
        let configured = has_mapping(cfg, port, https_port);
        let serve_only = !configured && has_serve_only_mapping(cfg, port, https_port);
        let url = if configured {
            extract_tailscale_url(cfg, port, https_port, true)
        } else {
            None
        };
        TunnelObservation {
            configured,
            online,
            url,
            note: if !online {
                "Tailscale 节点未在线".into()
            } else if serve_only {
                format!(
                    "检测到 HTTPS 端口 {https_port} 已配置 Tailscale Serve（仅内网）；如需仅 Tailnet 访问，请在连接页切换为 Tailscale Serve"
                )
            } else if !configured {
                "隧道映射与当前 API 不一致".into()
            } else {
                "隧道已配置；公网来源尚未验证".into()
            },
        }
    } else {
        let funnel_on = has_funnel_enabled_on_port(cfg, https_port);
        let configured = has_serve_only_mapping(cfg, port, https_port) && !funnel_on;
        let url = if configured {
            extract_tailscale_url(cfg, port, https_port, false)
        } else {
            None
        };
        TunnelObservation {
            configured,
            online,
            url,
            note: if !online {
                "Tailscale 节点未在线".into()
            } else if funnel_on {
                format!("HTTPS 端口 {https_port} 当前已开启公网 Funnel，与仅 Tailnet 内网模式不符")
            } else if !configured {
                "隧道映射与当前 API 不一致".into()
            } else {
                "Tailnet 内网转发已配置（不暴露公网）；尚未验证连接".into()
            },
        }
    }
}

fn proxy_matches_port(value: &Value, port: u16) -> bool {
    value
        .as_str()
        .is_some_and(|raw| raw.trim_end_matches('/') == format!("http://127.0.0.1:{port}"))
}

fn host_matches_https_port(host: &str, https_port: u16) -> bool {
    host.ends_with(&format!(":{https_port}"))
}

fn extract_tailscale_url(
    cfg: &Value,
    port: u16,
    https_port: u16,
    require_funnel: bool,
) -> Option<String> {
    cfg["Web"].as_object().and_then(|web| {
        web.iter()
            .find(|(host, v)| {
                host_matches_https_port(host, https_port)
                    && proxy_matches_port(&v["Handlers"]["/"]["Proxy"], port)
                    && if require_funnel {
                        cfg["AllowFunnel"][host.as_str()] == true
                    } else {
                        cfg["AllowFunnel"][host.as_str()] != true
                    }
            })
            .map(|(host, _)| {
                if https_port == 443 {
                    format!("https://{}", host.trim_end_matches(":443"))
                } else {
                    format!("https://{host}")
                }
            })
    })
}

pub fn validate_provider(settings: &ProviderSettings) -> Result<(), String> {
    match settings.kind {
        ProviderKind::Tailscale => {
            if !matches!(settings.https_port, 443 | 8443 | 10000) {
                return Err("Tailscale Funnel 仅支持 443、8443 或 10000 端口".into());
            }
        }
        ProviderKind::TailscaleServe => {
            if settings.https_port == 0 {
                return Err("请输入 1–65535 之间的有效 HTTPS 端口".into());
            }
        }
        ProviderKind::CloudflareNamed => {
            if settings.tunnel_id.is_empty()
                || settings.tunnel_id.len() > 100
                || !settings
                    .tunnel_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err("请输入有效的隧道 ID 或名称".into());
            }
            let hostname = &settings.hostname;
            if hostname.len() > 253
                || !hostname.contains('.')
                || !hostname.split('.').all(|s| {
                    !s.is_empty()
                        && s.len() <= 63
                        && !s.starts_with('-')
                        && !s.ends_with('-')
                        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                })
            {
                return Err("请输入不含协议、端口或路径的域名".into());
            }
        }
    }
    Ok(())
}
fn has_mapping(cfg: &Value, port: u16, https_port: u16) -> bool {
    let tcp_port = https_port.to_string();
    cfg["TCP"][&tcp_port]["HTTPS"] == true
        && cfg["Web"].as_object().is_some_and(|web| {
            web.iter().any(|(host, v)| {
                host_matches_https_port(host, https_port)
                    && proxy_matches_port(&v["Handlers"]["/"]["Proxy"], port)
                    && cfg["AllowFunnel"][host.as_str()] == true
            })
        })
}
fn has_serve_only_mapping(cfg: &Value, port: u16, https_port: u16) -> bool {
    let tcp_port = https_port.to_string();
    cfg["TCP"][&tcp_port]["HTTPS"] == true
        && cfg["Web"].as_object().is_some_and(|web| {
            web.iter().any(|(host, v)| {
                host_matches_https_port(host, https_port)
                    && proxy_matches_port(&v["Handlers"]["/"]["Proxy"], port)
                    && cfg["AllowFunnel"][host.as_str()] != true
            })
        })
}
fn has_funnel_enabled_on_port(cfg: &Value, https_port: u16) -> bool {
    cfg["AllowFunnel"].as_object().is_some_and(|allow| {
        allow
            .iter()
            .any(|(host, enabled)| host_matches_https_port(host, https_port) && *enabled == true)
    })
}
pub(crate) fn ensure_exclusive_mapping(
    cfg: &Value,
    port: u16,
    https_port: u16,
) -> Result<(), String> {
    let object = cfg
        .as_object()
        .ok_or("Tailscale 返回了未知结构，未改变转发")?;
    if !object.is_empty()
        && !["Web", "TCP", "AllowFunnel"]
            .iter()
            .any(|key| object.contains_key(*key))
    {
        return Err("Tailscale 配置结构未知，未改变转发".into());
    }
    if object.get("Web").is_some_and(|v| !v.is_object()) {
        return Err("Tailscale 路由结构无效".into());
    }
    if let Some(web) = cfg["Web"].as_object() {
        for (host, entry) in web {
            if !host_matches_https_port(host, https_port) {
                continue;
            }
            if let Some(handlers) = entry["Handlers"].as_object() {
                for (path, handler) in handlers {
                    if path != "/" || !proxy_matches_port(&handler["Proxy"], port) {
                        return Err(format!(
                            "HTTPS 端口 {https_port} 含其他应用的转发，已拒绝覆盖或撤销"
                        ));
                    }
                }
            } else {
                return Err(format!("HTTPS 端口 {https_port} 路由结构未知，未改变转发"));
            }
        }
    }
    let tcp_port = https_port.to_string();
    if cfg["TCP"][&tcp_port].get("TCPForward").is_some() {
        return Err(format!("HTTPS 端口 {https_port} 正被其他 TCP 转发使用"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn tailscale_bundle_path_points_into_the_app() {
        assert_eq!(
            tailscale_app_bundle_exe(),
            PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale")
        );
    }
    #[test]
    fn platform_exe_name_strips_exe_on_unix() {
        if cfg!(not(windows)) {
            assert_eq!(platform_exe_name("tailscale.exe"), "tailscale");
            assert_eq!(platform_exe_name("tailscale"), "tailscale");
            assert_eq!(platform_exe_name("cloudflared.exe"), "cloudflared");
        }
    }
    #[test]
    fn mapping_checks_target_and_rejects_foreign_handlers() {
        assert!(ensure_exclusive_mapping(&json!(null), 18790, 443).is_err());
        assert!(ensure_exclusive_mapping(&json!({"unexpected":true}), 18790, 443).is_err());
        assert!(ensure_exclusive_mapping(&json!({"Web":[]}), 18790, 443).is_err());
        let cfg = json!({"TCP":{"443":{"HTTPS":true}},"Web":{"example.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:18790"}}}},"AllowFunnel":{"example.ts.net:443":true}});
        assert!(has_mapping(&cfg, 18790, 443));
        assert!(!has_mapping(&cfg, 18795, 443));
        assert!(ensure_exclusive_mapping(&cfg, 18795, 443).is_err());
        let foreign = json!({"Web":{"example.ts.net:443":{"Handlers":{"/other":{"Proxy":"http://127.0.0.1:1234"}}}}});
        assert!(ensure_exclusive_mapping(&foreign, 18790, 443).is_err());
        // 443 被其他应用占用时，切换至 8443 互不冲突
        assert!(ensure_exclusive_mapping(&foreign, 18790, 8443).is_ok());

        let serve_only = json!({
            "TCP": {"443": {"HTTPS": true}},
            "Web": {"example.ts.net:443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:18790/"}}}}
        });
        assert!(ensure_exclusive_mapping(&serve_only, 18790, 443).is_ok());
        assert!(has_serve_only_mapping(&serve_only, 18790, 443));
        assert!(!has_mapping(&serve_only, 18790, 443));
        assert!(!has_funnel_enabled_on_port(&serve_only, 443));

        // Tailscale Serve 模式识别纯内网映射并提取 HTTPS 地址（443 省略端口）
        let serve_obs = evaluate_tailscale_status(&serve_only, true, 18790, 443, false);
        assert!(serve_obs.configured);
        assert!(serve_obs.online);
        assert_eq!(serve_obs.url.as_deref(), Some("https://example.ts.net"));
        assert!(serve_obs.note.contains("Tailnet"));

        // 自定义非 443 端口：与 443 上的其他服务共存，且 URL 保留端口号
        let serve_custom = json!({
            "TCP": {"443": {"HTTPS": true}, "8443": {"HTTPS": true}},
            "Web": {
                "example.ts.net:443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:3000"}}},
                "example.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:18790"}}}
            },
            "AllowFunnel": {"example.ts.net:443": true}
        });
        assert!(ensure_exclusive_mapping(&serve_custom, 18790, 8443).is_ok());
        assert!(!has_funnel_enabled_on_port(&serve_custom, 8443));
        let custom_obs = evaluate_tailscale_status(&serve_custom, true, 18790, 8443, false);
        assert!(custom_obs.configured);
        assert_eq!(
            custom_obs.url.as_deref(),
            Some("https://example.ts.net:8443")
        );

        // Tailscale Funnel 模式遇到仅 Serve 映射时，不判为公网就绪并引导切换
        let funnel_sees_serve = evaluate_tailscale_status(&serve_only, true, 18790, 443, true);
        assert!(!funnel_sees_serve.configured);
        assert_eq!(funnel_sees_serve.url, None);
        assert!(funnel_sees_serve.note.contains("Tailscale Serve"));

        // Tailscale Serve 模式遇到已开公网 Funnel 时拒绝当作纯内网
        let serve_sees_funnel = evaluate_tailscale_status(&cfg, true, 18790, 443, false);
        assert!(!serve_sees_funnel.configured);
        assert_eq!(serve_sees_funnel.url, None);
        assert!(serve_sees_funnel.note.contains("Funnel"));

        // 正常 Funnel 模式识别公网地址
        let funnel_obs = evaluate_tailscale_status(&cfg, true, 18790, 443, true);
        assert!(funnel_obs.configured);
        assert_eq!(funnel_obs.url.as_deref(), Some("https://example.ts.net"));

        // 提供方端口校验：Funnel 仅限 443 / 8443 / 10000，Serve 允许 1-65535
        for ok_port in [443, 8443, 10000] {
            assert!(
                validate_provider(&ProviderSettings {
                    kind: ProviderKind::Tailscale,
                    https_port: ok_port,
                    ..ProviderSettings::default()
                })
                .is_ok()
            );
        }
        assert!(
            validate_provider(&ProviderSettings {
                kind: ProviderKind::Tailscale,
                https_port: 8080,
                ..ProviderSettings::default()
            })
            .is_err()
        );
        assert!(
            validate_provider(&ProviderSettings {
                kind: ProviderKind::TailscaleServe,
                https_port: 18443,
                ..ProviderSettings::default()
            })
            .is_ok()
        );
        assert!(
            validate_provider(&ProviderSettings {
                kind: ProviderKind::TailscaleServe,
                https_port: 0,
                ..ProviderSettings::default()
            })
            .is_err()
        );
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn closing_our_job_terminates_our_child() {
        let system =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe");
        let mut child = command(&system)
            .args(["-n", "120", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let job = crate::core::job::ChildJob::attach_tokio(&child).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        drop(job);
        // Windows may use exit code 0 when closing a kill-on-close job.
        // A 120-second child finishing within this bound proves containment.
        let _result = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
    }
}
