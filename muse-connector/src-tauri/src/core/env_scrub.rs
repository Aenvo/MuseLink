//! 子进程环境变量清洗：判断某个环境变量名是否像凭证 / 密钥。
//!
//! 受控命令执行（exec）会继承连接器进程的环境变量，其中可能包含
//! `GITHUB_TOKEN`、`AWS_SECRET_ACCESS_KEY` 等与本次任务无关的凭证。
//! 本模块只按「变量名」判定敏感与否，绝不读取、记录或比较变量值，
//! 判定逻辑为纯函数且与平台无关，便于在任意平台单测。

/// 已知的敏感变量名（精确匹配，已统一为大写）。
///
/// 其中不少条目同时也会被下方的子串规则命中，保留在此是为了
/// 让清单本身可读、可审计，并覆盖个别不含敏感子串的命名。
const EXACT_SENSITIVE_NAMES: &[&str] = &[
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_CONTAINER_CREDENTIALS",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "AZURE_CLIENT_SECRET",
    "NPM_TOKEN",
    "NUGET_API_KEY",
    "PYPI_TOKEN",
    "DOCKER_PASSWORD",
    "HF_TOKEN",
    "HUGGING_FACE_HUB_TOKEN",
    "SLACK_TOKEN",
    "DISCORD_TOKEN",
    "TELEGRAM_BOT_TOKEN",
];

/// 变量名中出现任一子串即视为敏感（已统一为大写后比较）。
const SENSITIVE_SUBSTRINGS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "APIKEY",
    "PRIVATE_KEY",
    "ACCESS_KEY",
    "CREDENTIAL",
];

/// 判断环境变量名是否像凭证 / 密钥（大小写不敏感）。
///
/// 仅依据名称判定：命中精确清单，或名称包含任一敏感子串即为敏感。
/// 进程运行所必需的常规变量（`PATH`、`SYSTEMROOT`、`TEMP`、
/// `USERPROFILE` 等）不含上述子串，不会被误杀。
pub(crate) fn is_sensitive_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if EXACT_SENSITIVE_NAMES.contains(&upper.as_str()) {
        return true;
    }
    SENSITIVE_SUBSTRINGS.iter().any(|s| upper.contains(s))
}

#[cfg(test)]
mod tests {
    use super::is_sensitive_env_name;

    #[test]
    fn flags_sensitive_names_case_insensitively() {
        for name in [
            // 精确清单（含混合大小写）
            "GITHUB_TOKEN",
            "github_token",
            "GH_TOKEN",
            "Aws_Secret_Access_Key",
            "AWS_SESSION_TOKEN",
            "OPENAI_API_KEY",
            "Anthropic_Api_Key",
            "AZURE_CLIENT_SECRET",
            "NPM_TOKEN",
            "HF_TOKEN",
            "Slack_Token",
            "TELEGRAM_BOT_TOKEN",
            // 子串规则
            "MY_API_KEY",
            "app_password",
            "APP_PASSWORD",
            "X_PRIVATE_KEY_PEM",
            "SOME_SECRET",
            "DB_PASSWD",
            "SERVICE_CREDENTIALS",
            "MY_ACCESS_KEY_ID",
            "custom_apikey",
            "REFRESH_TOKEN",
        ] {
            assert!(is_sensitive_env_name(name), "应判定为敏感: {name}");
        }
    }

    #[test]
    fn keeps_essential_and_ordinary_names() {
        for name in [
            // Windows / 进程运行必需变量
            "PATH",
            "Path",
            "SYSTEMROOT",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "USERNAME",
            "COMPUTERNAME",
            "COMSPEC",
            "PATHEXT",
            "APPDATA",
            "LOCALAPPDATA",
            "PROGRAMFILES",
            // 常见开发 / 终端变量
            "HOME",
            "LANG",
            "TERM",
            "CARGO",
            "RUSTUP_HOME",
            "NODE_ENV",
            "EDITOR",
        ] {
            assert!(!is_sensitive_env_name(name), "不应判定为敏感: {name}");
        }
    }
}
