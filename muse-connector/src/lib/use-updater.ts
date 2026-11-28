import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { useLanguage } from "@/lib/i18n";
import type { Snapshot } from "@/lib/use-bridge";
import {
  FOREGROUND_AUTO_CHECK_DELAY_MS,
  computeNextAutoCheckDelayMs,
  resolvePhaseAfterSilentFailure,
  type AutoCheckScheduleState,
  type UpdaterVisualPhase,
} from "@/lib/updater-schedule";

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

export type UpdatePhase = UpdaterVisualPhase;

export function useUpdater(snapshot: Snapshot | null, windowVisible: boolean) {
  const { t, diagnostic } = useLanguage();
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [result, setResult] = useState<UpdateCheckResult | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const [scheduleVersion, setScheduleVersion] = useState(0);

  const mountedRef = useRef(true);
  const tRef = useRef(t);
  tRef.current = t;
  const diagnosticRef = useRef(diagnostic);
  diagnosticRef.current = diagnostic;

  const inFlightCheckRef = useRef(false);
  const inFlightPromiseRef = useRef<Promise<void> | null>(null);
  const manualRequestRef = useRef(false);
  const installingRef = useRef(false);
  const hasKnownUpdateRef = useRef(false);
  const scheduleRef = useRef<AutoCheckScheduleState>({
    lastAttemptAt: null,
    lastOutcome: null,
    retryOnOnlinePending: false,
  });

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

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
    if (!isTauri() || installingRef.current) return;
    if (inFlightCheckRef.current) {
      if (manual) {
        // 静默检查在途时用户点击手动检查，直接复用在途请求并升级为手动反馈
        manualRequestRef.current = true;
        setPhase("checking");
        setErrorMessage(null);
        await inFlightPromiseRef.current;
      }
      return;
    }

    inFlightCheckRef.current = true;
    manualRequestRef.current = manual;
    scheduleRef.current = {
      ...scheduleRef.current,
      retryOnOnlinePending: false,
    };
    if (manual) {
      setPhase("checking");
      setErrorMessage(null);
    }

    const task = (async () => {
      try {
        const checked = await invoke<UpdateCheckResult>("check_updates");
        scheduleRef.current = {
          lastAttemptAt: Date.now(),
          lastOutcome: "success",
          retryOnOnlinePending: false,
        };
        if (!mountedRef.current || installingRef.current) return;
        const isManual = manualRequestRef.current;
        const available = Boolean(checked.available && checked.version);
        hasKnownUpdateRef.current = available;
        setResult(checked);
        if (available) {
          setPhase("available");
          setErrorMessage(null);
          if (isManual) setDismissedVersion(null);
        } else {
          setPhase("up-to-date");
          setErrorMessage(null);
          if (isManual) toast.success(tRef.current("当前已是最新版本"));
        }
      } catch (error) {
        scheduleRef.current = {
          lastAttemptAt: Date.now(),
          lastOutcome: "failure",
          retryOnOnlinePending: false,
        };
        if (!mountedRef.current || installingRef.current) return;
        const isManual = manualRequestRef.current;
        if (isManual) {
          setPhase("error");
          setErrorMessage(diagnosticRef.current(String(error)));
        } else {
          // 启动或后台静默复查失败时不打扰用户，也绝不擦除此前已确认的新版本提示
          setPhase(previous => resolvePhaseAfterSilentFailure(previous, hasKnownUpdateRef.current));
        }
      } finally {
        inFlightCheckRef.current = false;
        manualRequestRef.current = false;
        inFlightPromiseRef.current = null;
        if (mountedRef.current) {
          setScheduleVersion(version => version + 1);
        }
      }
    })();

    inFlightPromiseRef.current = task;
    await task;
  }, []);

  const autoCheckEligible = Boolean(
    snapshot && (
      snapshot.profile === "production"
      || snapshot.in_app_update_supported
      || snapshot.auto_update_check_enabled
    ),
  );

  // 仅依赖稳定的资格标记、窗口可见性与排程版本号，避免 snapshot 高频刷新或语言切换误取消定时器。
  useEffect(() => {
    if (!isTauri() || !autoCheckEligible || !windowVisible) return;
    if (inFlightCheckRef.current || installingRef.current) return;
    const delay = computeNextAutoCheckDelayMs(scheduleRef.current, Date.now());
    const timer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, delay);
    return () => window.clearTimeout(timer);
  }, [autoCheckEligible, windowVisible, scheduleVersion, checkForUpdates]);

  // 窗口可见期间监听网络恢复（提前补查失败的静默请求）与焦点恢复（应对系统休眠跨越冷却期）。
  useEffect(() => {
    if (!isTauri() || !autoCheckEligible || !windowVisible) return;
    const onOnline = () => {
      if (inFlightCheckRef.current || installingRef.current) return;
      if (scheduleRef.current.lastOutcome === "failure") {
        scheduleRef.current = {
          ...scheduleRef.current,
          retryOnOnlinePending: true,
        };
        setScheduleVersion(version => version + 1);
      }
    };
    const onFocus = () => {
      if (inFlightCheckRef.current || installingRef.current) return;
      if (scheduleRef.current.lastAttemptAt === null) return;
      if (computeNextAutoCheckDelayMs(scheduleRef.current, Date.now()) <= FOREGROUND_AUTO_CHECK_DELAY_MS) {
        setScheduleVersion(version => version + 1);
      }
    };
    window.addEventListener("online", onOnline);
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("focus", onFocus);
    };
  }, [autoCheckEligible, windowVisible]);

  const installUpdate = useCallback(async () => {
    if (!isTauri() || installingRef.current) return false;
    installingRef.current = true;
    setErrorMessage(null);
    setProgress({ stage: "downloading", downloaded: 0, total: null });
    setPhase("downloading");
    try {
      await invoke("install_update");
      // Windows NSIS passive 模式会在 Rust 侧自动退出并拉起安装器重启；
      // 若命令正常返回，保持 installing 态等待进程交接。
      if (mountedRef.current) setPhase("installing");
      return true;
    } catch (error) {
      installingRef.current = false;
      if (mountedRef.current) {
        setPhase("error");
        setErrorMessage(diagnosticRef.current(String(error)));
        setScheduleVersion(version => version + 1);
      }
      return false;
    }
  }, []);

  const openReleases = useCallback(async () => {
    try {
      await openUrl(result?.releases_url ?? RELEASES_URL);
    } catch {
      toast.error(tRef.current("无法打开发布页面，请稍后重试。"));
    }
  }, [result?.releases_url]);

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
