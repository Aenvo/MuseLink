import { createContext, useCallback, useContext, useEffect, type ReactNode } from "react";
import { english, japanese } from "@/lib/messages";

export type Language = "zh-CN" | "en" | "ja";
type Message = keyof typeof english;
export type Translate = (message: Message, ...values: (string | number)[]) => string;

export function detectBrowserLanguage(): Language {
  if (typeof navigator === "undefined") return "en";
  const candidates = navigator.languages?.length ? navigator.languages : navigator.language ? [navigator.language] : [];
  for (const raw of candidates) {
    const primary = raw.trim().split(/[.@]/)[0]?.split(/[-_]/)[0]?.toLowerCase();
    if (primary === "zh" || primary === "zho" || primary === "chi") return "zh-CN";
    if (primary === "ja" || primary === "jpn") return "ja";
    if (primary === "en" || primary === "eng" || primary === "c" || primary === "posix") return "en";
  }
  return "en";
}

const LanguageContext = createContext<Language>("en");
const catalogs: Record<Exclude<Language, "zh-CN">, Record<Message, string>> = { en: english, ja: japanese };
const diagnosticPatterns = [
  [/^读取连接器状态失败：([\s\S]+)$/, "读取连接器状态失败：{0}"],
  [/^本地 API 正常，隧道不可用：([\s\S]+)$/, "本地 API 正常，隧道不可用：{0}"],
  [/^文件服务无法监听：([\s\S]+)$/, "文件服务无法监听：{0}"],
  [/^文件服务已停止，隧道清理失败：([\s\S]+)$/, "文件服务已停止，隧道清理失败：{0}"],
  [/^端口 (\d+) 被其他进程占用；本应用未取得服务归属$/, "端口 {0} 被其他进程占用；本应用未取得服务归属"],
  [/^检测到 HTTPS 端口 (\d+) 已配置 Tailscale Serve（仅内网），已拒绝自动开启公网 Funnel；如需仅 Tailnet 内网访问，请在连接页切换为 Tailscale Serve$/, "检测到 HTTPS 端口 {0} 已配置 Tailscale Serve（仅内网），已拒绝自动开启公网 Funnel；如需仅 Tailnet 内网访问，请在连接页切换为 Tailscale Serve"],
  [/^HTTPS 端口 (\d+) 当前已开启公网 Tailscale Funnel，已拒绝按仅内网模式接管；请先关闭 Funnel 或切换至 Tailscale Funnel 模式$/, "HTTPS 端口 {0} 当前已开启公网 Tailscale Funnel，已拒绝按仅内网模式接管；请先关闭 Funnel 或切换至 Tailscale Funnel 模式"],
  [/^检测到 HTTPS 端口 (\d+) 已配置 Tailscale Serve（仅内网）；如需仅 Tailnet 访问，请在连接页切换为 Tailscale Serve$/, "检测到 HTTPS 端口 {0} 已配置 Tailscale Serve（仅内网）；如需仅 Tailnet 访问，请在连接页切换为 Tailscale Serve"],
  [/^HTTPS 端口 (\d+) 当前已开启公网 Funnel，与仅 Tailnet 内网模式不符$/, "HTTPS 端口 {0} 当前已开启公网 Funnel，与仅 Tailnet 内网模式不符"],
  [/^HTTPS 端口 (\d+) 含其他应用的转发，已拒绝覆盖或撤销$/, "HTTPS 端口 {0} 含其他应用的转发，已拒绝覆盖或撤销"],
  [/^HTTPS 端口 (\d+) 路由结构未知，未改变转发$/, "HTTPS 端口 {0} 路由结构未知，未改变转发"],
  [/^HTTPS 端口 (\d+) 正被其他 TCP 转发使用$/, "HTTPS 端口 {0} 正被其他 TCP 转发使用"],
] as const satisfies readonly (readonly [RegExp, Message])[];

export function LanguageProvider({ language, children }: { language: Language; children: ReactNode }) {
  useEffect(() => {
    document.documentElement.lang = language;
    document.title = language === "zh-CN" ? "Muse连接器" : catalogs[language]["Muse连接器"];
  }, [language]);

  return <LanguageContext.Provider value={language}>{children}</LanguageContext.Provider>;
}

export function useLanguage() {
  const language = useContext(LanguageContext);
  const t = useCallback<Translate>((message, ...values) => {
    const pattern = language === "zh-CN" ? message : catalogs[language][message];
    return values.length ? pattern.replace(/\{(\d+)\}/g, (placeholder, index) => String(values[Number(index)] ?? placeholder)) : pattern;
  }, [language]);

  const diagnostic = useCallback(function formatDiagnostic(message: string): string {
    if (language === "zh-CN") return message;
    if (Object.prototype.hasOwnProperty.call(catalogs[language], message)) return t(message as Message);
    for (const [pattern, key] of diagnosticPatterns) {
      const match = pattern.exec(message);
      if (match) return t(key, formatDiagnostic(match[1]));
    }
    return message;
  }, [language, t]);

  return { language, t, diagnostic };
}
