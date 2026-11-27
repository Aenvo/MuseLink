import { Download, ExternalLink, RefreshCw } from "lucide-react";
import { useLanguage } from "@/lib/i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import appIcon from "../../src-tauri/icons/128x128.png";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { formatBytes } from "@/lib/use-bridge";
import type { UpdaterModel } from "@/lib/use-updater";

const githubRepositoryUrl = "https://github.com/Aenvo/MuseLink";

export default function About({
  version,
  updater,
  onOpenChangelog,
  onTriggerUpdate,
  onError,
}: {
  version: string;
  updater: UpdaterModel;
  onOpenChangelog: () => void;
  onTriggerUpdate: () => void;
  onError: (message: string) => void;
}) {
  const { t } = useLanguage();
  const { phase, result, progress, errorMessage, busy } = updater;

  const openRepository = async () => {
    try {
      await openUrl(githubRepositoryUrl);
    } catch {
      onError(t("无法打开 GitHub 仓库，请稍后重试。"));
    }
  };

  const installing = phase === "downloading" || phase === "verifying" || phase === "installing";
  const showUpdatePanel = Boolean(
    (result?.available && result.version)
    || installing
    || (phase === "error" && errorMessage),
  );
  const percent = progress?.total && progress.total > 0
    ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
    : null;

  const nonInAppHint = result?.install_kind === "portable"
    ? t("当前为免安装便携版，请前往 GitHub Releases 下载新版压缩包覆盖升级。")
    : result?.install_kind === "linux"
      ? t("当前为 Linux 安装包或源码构建，请前往 GitHub Releases 下载新版（AppImage 支持应用内更新）。")
      : result?.install_kind === "development"
        ? t("开发隔离模式不执行正式安装包更新。")
        : t("当前平台暂不支持应用内覆盖安装，请前往 GitHub Releases 下载新版。");

  return (
    <Card className="gap-0 overflow-hidden rounded-[14px] py-0 max-[600px]:py-0" aria-labelledby="about-title">
      <CardHeader className="flex flex-col gap-4 px-6 py-5">
        <div className="flex w-full flex-wrap items-center justify-between gap-x-6 gap-y-3">
          <div className="flex min-w-0 items-center gap-3">
            <img src={appIcon} alt="" width={48} height={48} className="size-12 shrink-0" />
            <div className="min-w-0">
              <CardTitle id="about-title">{t("Muse连接器")}</CardTitle>
              <CardDescription>{t("版本 v{0}", version)}</CardDescription>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onOpenChangelog}>
              {t("更新日志")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void openRepository()}>
              {t("GitHub")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => void updater.checkForUpdates(true)}
            >
              {phase === "checking" ? <Spinner data-icon="inline-start" aria-hidden="true" role="presentation" /> : null}
              {phase === "checking" ? t("检查中…") : t("检查更新")}
            </Button>
          </div>
        </div>

        {showUpdatePanel ? (
          <>
            <Separator />
            {phase === "error" && errorMessage ? (
              <div className="flex w-full flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <p role="alert" className="text-caption text-status-error">{errorMessage}</p>
                <div className="flex shrink-0 flex-wrap items-center gap-2">
                  <Button variant="ghost" size="sm" onClick={() => void updater.openReleases()}>
                    <ExternalLink data-icon="inline-start" aria-hidden="true" />
                    {t("前往下载")}
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => void updater.checkForUpdates(true)}>
                    <RefreshCw data-icon="inline-start" aria-hidden="true" />
                    {t("重试")}
                  </Button>
                </div>
              </div>
            ) : result?.version ? (
              <div className="flex w-full flex-col gap-3">
                <div className="flex w-full flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <p className="text-sm leading-[22px] font-medium text-foreground">
                      {t("发现新版本 v{0}", result.version)}
                    </p>
                    <p className="text-caption leading-[21px] text-muted-foreground">
                      {installing
                        ? phase === "verifying"
                          ? t("正在校验更新签名…")
                          : phase === "installing"
                            ? t("正在停止服务并准备重启更新…")
                            : percent != null && progress?.total
                              ? t("正在下载更新 · {0} / {1} ({2}%)", formatBytes(progress.downloaded), formatBytes(progress.total), percent)
                              : t("正在下载更新 · {0}", formatBytes(progress?.downloaded ?? 0))
                        : result.can_install_in_app
                          ? t("下载和校验期间保持连接；校验通过后会自动重启")
                          : nonInAppHint}
                    </p>
                  </div>
                  <div className="flex shrink-0 flex-wrap items-center gap-2">
                    {result.can_install_in_app ? (
                      <Button variant="default" size="sm" className="gap-2 px-3" disabled={installing} onClick={onTriggerUpdate}>
                        <Download className="size-4" aria-hidden="true" />
                        {installing ? t("更新中…") : t("立即更新")}
                      </Button>
                    ) : (
                      <Button variant="default" size="sm" className="gap-2 px-3" onClick={() => void updater.openReleases()}>
                        <ExternalLink className="size-4" aria-hidden="true" />
                        {t("前往下载")}
                      </Button>
                    )}
                  </div>
                </div>
                {installing ? (
                  <Progress value={phase === "verifying" || phase === "installing" ? 100 : (percent ?? 15)} />
                ) : null}
              </div>
            ) : null}
          </>
        ) : null}
      </CardHeader>

      <Separator />
      <CardFooter className="bg-muted px-6 py-4">
        <p>{t("如果 Muse连接器帮到了你，欢迎在 GitHub 点个 Star :)")}</p>
      </CardFooter>
    </Card>
  );
}
