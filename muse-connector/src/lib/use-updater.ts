import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { useLanguage } from "@/lib/i18n";
import type { Snapshot } from "@/lib/use-bridge";

export const RELEASES_URL = "https://github.com/Aenvo/MuseLink/releases/latest";

export interface UpdateCheckResult {
  configured: boolean;
  available: boolean;
  current_version: string;
  version: string | null;
  notes: string | null;
  date: string | null;
  can_install_in_app: boolean;
  install_kind: string;
  releases_url: string;
  message: string | null;
}

export interface UpdateProgress {
  stage: "downloading" | "verifying" | "installing" | string;
  downloaded: number;
  total: number | null;
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "verifying"
  | "installing"
  | "error";

export function useUpdater(snapshot: Snapshot | null) {
  const { t, diagnostic } = useLanguage();
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [result, setResult] = useState<UpdateCheckResult | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const autoChecked = useRef(false);
  const phaseRef = useRef<UpdatePhase>("idle");
  phaseRef.current = phase;

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const subscription = listen<UpdateProgress>("update-progress", ({ payload }) => {
      if (disposed || !payload || typeof payload !== "object") return;
      setProgress(payload);
      if (payload.stage === "verifying") setPhase("verifying");
      else if (payload.stage === "installing") setPhase("installing");
      else setPhase("downloading");
    }).catch(() => undefined);
    return () => {
      disposed = true;
      void subscription.then(stop => stop?.());
    };
  }, []);

  const checkForUpdates = useCallback(async (manual: boolean) => {
    if (!isTauri()) return;
    if (
      phaseRef.current === "checking"
      || phaseRef.current === "downloading"
      || phaseRef.current === "verifying"
      || phaseRef.current === "installing"
    ) {
      return;
    }
    setPhase("checking");
    if (manual) setErrorMessage(null);
    try {
      const checked = await invoke<UpdateCheckResult>("check_updates");
      setResult(checked);
      if (checked.available && checked.version) {
        setPhase("available");
        setErrorMessage(null);
        if (manual) setDismissedVersion(null);
      } else {
        setPhase("up-to-date");
        setErrorMessage(null);
        if (manual) toast.success(t("当前已是最新版本"));
      }
    } catch (error) {
      const message = diagnostic(String(error));
      if (manual) {
        setPhase("error");
        setErrorMessage(message);
      } else {
        // 启动静默检查失败时不打扰用户
        setPhase("idle");
      }
    }
  }, [diagnostic, t]);

  useEffect(() => {
    if (!isTauri() || !snapshot || autoChecked.current) return;
    if (snapshot.profile !== "production" && !snapshot.in_app_update_supported) return;
    autoChecked.current = true;
    const timer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [snapshot, checkForUpdates]);

  const installUpdate = useCallback(async () => {
    if (!isTauri()) return false;
    if (
      phaseRef.current === "downloading"
      || phaseRef.current === "verifying"
      || phaseRef.current === "installing"
    ) {
      return false;
    }
    setErrorMessage(null);
    setProgress({ stage: "downloading", downloaded: 0, total: null });
    setPhase("downloading");
    try {
      await invoke("install_update");
      // Windows NSIS passive 模式会在 Rust 侧自动退出并拉起安装器重启；
      // 若命令正常返回，保持 installing 态等待进程交接。
      setPhase("installing");
      return true;
    } catch (error) {
      const message = diagnostic(String(error));
      setPhase("error");
      setErrorMessage(message);
      return false;
    }
  }, [diagnostic]);

  const openReleases = useCallback(async () => {
    try {
      await openUrl(result?.releases_url ?? RELEASES_URL);
    } catch {
      toast.error(t("无法打开发布页面，请稍后重试。"));
    }
  }, [result?.releases_url, t]);

  const dismissNotice = useCallback(() => {
    if (result?.version) setDismissedVersion(result.version);
  }, [result?.version]);

  const busy = phase === "checking" || phase === "downloading" || phase === "verifying" || phase === "installing";
  const hasUpdate = Boolean(result?.available && result.version);
  const noticeVisible = Boolean(
    hasUpdate
    && result?.version
    && dismissedVersion !== result.version
    && (phase === "available" || phase === "downloading" || phase === "verifying" || phase === "installing"),
  );

  return {
    phase,
    result,
    progress,
    errorMessage,
    busy,
    hasUpdate,
    noticeVisible,
    checkForUpdates,
    installUpdate,
    openReleases,
    dismissNotice,
  };
}

export type UpdaterModel = ReturnType<typeof useUpdater>;
