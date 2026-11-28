import { X } from "lucide-react";
import { useLanguage } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { formatBytes } from "@/lib/use-bridge";
import type { UpdaterModel } from "@/lib/use-updater";

export default function UpdateNotice({
  updater,
  suspended,
  onOpenReleaseDetails,
  onTriggerUpdate,
}: {
  updater: UpdaterModel;
  suspended: boolean;
  onOpenReleaseDetails: () => void;
  onTriggerUpdate: () => void;
}) {
  const { t } = useLanguage();
  const { result, phase, progress, noticeVisible, dismissNotice } = updater;
  if (!noticeVisible || !result?.version) return null;

  const installing = phase === "downloading" || phase === "verifying" || phase === "installing";
  const percent = progress?.total && progress.total > 0
    ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
    : null;

  return (
    <aside
      className="update-notice"
      aria-label={t("新版本提示")}
      hidden={suspended}
    >
      <Card className="gap-0 overflow-hidden rounded-[14px] py-0 shadow-[0_10px_24px_rgba(0,0,0,0.45)] max-[600px]:py-0">
        <CardHeader className="flex flex-row items-center justify-between gap-2 border-b py-[10px] pl-4 pr-3">
          <CardTitle className="text-sm leading-[22px] font-medium">
            {t("发现新版本 v{0}", result.version)}
          </CardTitle>
          <CardAction>
            <Button
              size="icon-sm"
              variant="ghost"
              disabled={installing}
              className="size-6 text-muted-foreground hover:text-foreground"
              aria-label={t("关闭新版本提示")}
              onClick={dismissNotice}
            >
              <X className="size-4" />
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent className="flex flex-col gap-2 px-4 py-3 text-caption leading-[21px] text-muted-foreground">
          {installing ? (
            <>
              <p className="leading-5 text-foreground">
                {phase === "verifying"
                  ? t("正在校验更新签名…")
                  : phase === "installing"
                    ? t("正在停止服务并准备重启更新…")
                    : percent != null && progress?.total
                      ? t("正在下载更新 · {0} / {1} ({2}%)", formatBytes(progress.downloaded), formatBytes(progress.total), percent)
                      : t("正在下载更新 · {0}", formatBytes(progress?.downloaded ?? 0))}
              </p>
              <Progress value={phase === "verifying" || phase === "installing" ? 100 : (percent ?? 15)} />
            </>
          ) : result.can_install_in_app ? (
            <p>{t("点击立即更新将下载并验证新版安装包，随后自动重启完成升级。")}</p>
          ) : (
            <p>{t("当前版本请前往 GitHub Releases 下载新版覆盖升级。")}</p>
          )}
        </CardContent>
        <CardFooter className="flex flex-wrap justify-end gap-2 px-3 py-2">
          <Button variant="ghost" size="sm" onClick={onOpenReleaseDetails}>
            {t("更新详情")}
          </Button>
          {result.can_install_in_app ? (
            <Button variant="default" size="sm" disabled={installing} onClick={onTriggerUpdate}>
              {installing ? t("更新中…") : t("立即更新")}
            </Button>
          ) : (
            <Button variant="default" size="sm" onClick={() => void updater.openReleases()}>
              {t("前往下载")}
            </Button>
          )}
        </CardFooter>
      </Card>
    </aside>
  );
}
