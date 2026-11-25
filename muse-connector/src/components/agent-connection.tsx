import { useLanguage, type Translate } from "@/lib/i18n";
import { useEffect, useState } from "react";
import { Clipboard, Copy, FileKey } from "lucide-react";
import type { Confirmation } from "@/components/feedback";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { providerToolAvailable, publicAddress } from "@/lib/setup-progress";
import { agentCredentialIssue, agentTrial, connectorBrief, hasFileGrant } from "@/lib/agent-setup";
import type { DeviceNameModel } from "@/lib/use-device-name";
import type { BridgeModel, Snapshot } from "@/lib/use-bridge";

const credentialActions = (t: Translate) => ({
  issue: agentCredentialIssue(t),
  rotate: {
    title: t("轮换 Muse 专用令牌？"),
    description: t("当前 Muse 专用令牌会立刻失效。轮换后请更新 Muse 安全卡片中的令牌。"),
    action: t("轮换令牌"),
    busy: t("正在轮换 Muse 专用令牌"),
    done: t("Muse 专用令牌已轮换"),
  },
  revoke: {
    title: t("撤销 Muse 专用令牌？"),
    description: t("现有 Muse 文件工具的令牌会立即失效。文件权限及旧版 File Bridge 连接仍可单独管理。"),
    action: t("撤销令牌"),
    busy: t("正在撤销 Muse 专用令牌"),
    done: t("Muse 专用令牌已撤销"),
  },
});

export default function AgentConnection({ bridge, snapshot, deviceName, onEditDeviceName, confirm }: {
  bridge: BridgeModel;
  snapshot: Snapshot;
  deviceName: DeviceNameModel;
  onEditDeviceName: () => void;
  confirm: (value: Confirmation) => void;
}) {
  const { language, t } = useLanguage();
  const [clipboard, setClipboard] = useState<{ instructions: string; version: string | null; copied: boolean } | null>(null);
  const [tokenCopyFailure, setTokenCopyFailure] = useState<{ version: string | null } | null>(null);
  const { file_access: policy, agent_ready: ready, agent_enabled: enabled, agent_token_version: tokenVersion, exec_enabled: execEnabled, exec_sessions: execSessions } = snapshot;
  const hasPermission = hasFileGrant(policy);
  const canIssue = hasPermission && !snapshot.setup_error;
  const origin = publicAddress(snapshot);
  const instructions = ready && origin && deviceName.canCopy ? connectorBrief(language, origin, agentTrial(t, policy).instruction, deviceName.saved, execEnabled) : null;
  const version = tokenVersion ?? null;
  const copyResult = instructions && clipboard?.instructions === instructions && clipboard.version === version ? clipboard : null;
  const busy = !!bridge.busy;
  const development = snapshot.profile === "development";
  const providerAvailable = providerToolAvailable(snapshot);

  useEffect(() => setClipboard(null), [deviceName.saved]);

  const changeCredential = (operation: keyof ReturnType<typeof credentialActions>) => {
    const details = credentialActions(t)[operation];
    confirm({ title: details.title, description: details.description, action: details.action, execute: async () => {
      const result = await bridge.run(details.busy, "set_agent_access", { enabled: operation !== "revoke" }, details.done);
      if (result.ok) { setClipboard(null); setTokenCopyFailure(null); }
      return result.ok;
    } });
  };

  const copy = async () => {
    if (!instructions) return;
    try {
      await navigator.clipboard.writeText(instructions);
      setClipboard({ instructions, version, copied: true });
    } catch {
      setClipboard({ instructions, version, copied: false });
    }
  };

  const copyToken = async () => {
    setTokenCopyFailure(null);
    const result = await bridge.run(t("正在复制专用令牌"), "copy_agent_token", undefined, t("令牌已复制，请粘贴到 Muse 安全卡片"), false);
    if (!result.ok) setTokenCopyFailure({ version });
  };

  return <section id="muse-agent-connection" tabIndex={-1} aria-labelledby="muse-agent-heading" className="flex min-w-0 flex-col gap-3 rounded-sm focus-visible:outline focus-visible:outline-ring">
    <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
      <h3 id="muse-agent-heading" className="text-sm font-medium">{t("Muse 文件工具")}</h3>
      <Badge variant="outline" role="status">{ready ? t("专用令牌已生成") : enabled ? t("当前授权不可用") : t("未生成专用令牌")}</Badge>
    </div>
    {!enabled ? <>
      <p className="text-caption text-muted-foreground">{!policy ? t("保存上方的文件授权后，即可生成 Muse 专用令牌。") : !hasPermission ? t("保存至少一项访问权限后，即可生成 Muse 专用令牌。") : t("授权已保存。生成专用令牌后，可把上方权限接入 Muse。")}</p>
      <Button size="sm" className="self-start" disabled={busy || !canIssue} onClick={() => changeCredential("issue")}>{t("生成 Muse 专用令牌")}</Button>
    </> : <>
      <p className="text-caption text-muted-foreground">{!ready ? t("请检查上方已保存的文件授权。") : development ? t("此环境使用独立的测试令牌与模拟连接。") : !origin ? (providerAvailable ? t("在右上角开启连接。连接就绪后即可复制接入说明。") : t("请先在「连接」页安装并登录所选公网转发工具。")) : deviceName.canCopy ? t("把接入说明发给 Muse。打开 Muse 的安全卡片后，复制专用令牌并粘贴到卡片中。") : t(deviceName.dirty ? "请先保存或取消设备名称的更改。" : "请先保存设备名称，再复制接入说明。")}</p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={busy || !instructions} onClick={() => void copy()}><Clipboard data-icon="inline-start" />{t("复制 Muse 接入说明")}</Button>
        {ready && origin && !deviceName.canCopy ? <Button size="sm" variant="outline" onClick={onEditDeviceName}>{t("设置设备名称")}</Button> : null}
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void copyToken()}><Copy data-icon="inline-start" />{t("复制专用令牌")}</Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void bridge.run(t("正在定位令牌文件"), "open_agent_token", undefined, undefined, false)}><FileKey data-icon="inline-start" />{t("定位令牌文件")}</Button>
        <Button size="sm" variant="ghost" disabled={busy || !canIssue} onClick={() => changeCredential("rotate")}>{t("轮换令牌")}</Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => changeCredential("revoke")}>{t("撤销令牌")}</Button>
      </div>
      {tokenCopyFailure?.version === version ? <p className="text-caption" role="alert">{t("复制令牌失败。请重试，或使用「定位令牌文件」在本机打开后复制。")}</p> : null}
      {copyResult?.copied ? <p className="text-caption text-muted-foreground" role="status">{t("说明已复制，可以发给 Muse。")}</p> : copyResult ? <div className="flex flex-col gap-2">
        <label className="text-caption text-muted-foreground" htmlFor="muse-agent-instructions">{t("请手动复制这段接入说明。")}</label>
        <textarea id="muse-agent-instructions" className="max-h-48 min-h-24 w-full resize-y rounded-md border bg-background p-2 text-caption" value={copyResult.instructions} readOnly onFocus={event => event.currentTarget.select()} />
      </div> : null}
      <Separator className="my-1" />
      <div className="flex flex-col gap-2 rounded-md border p-3">
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            <div className="flex flex-wrap items-center gap-2">
              <label htmlFor="exec-enabled-switch" className="text-sm font-medium text-foreground">{t("允许 Muse 执行本机命令")}</label>
              <Badge variant="outline">{execEnabled ? t("写受限沙箱") : t("已关闭")}</Badge>
            </div>
            <p className="text-caption text-muted-foreground">
              {t("开启后，Muse 可在已授权的非敏感项目文件夹内调用常见开发工具（如 git、cargo、pnpm、python）以及受控 PowerShell/cmd。命令默认运行在 Windows 写受限沙箱中（仅允许写入当前项目目录与隔离临时目录，且受内存、进程数与超时约束）；首次在项目内使用 Shell、安装依赖、推送远端、触碰敏感目录或请求跳出沙箱时，本机将弹出审批窗由你确认。关闭开关会立即停用并终止运行中的命令。")}
            </p>
          </div>
          <Switch
            id="exec-enabled-switch"
            checked={execEnabled}
            disabled={busy}
            onCheckedChange={(next) => {
              if (next) {
                confirm({
                  title: t("开启 Muse 本机命令执行？"),
                  description: t("开启后，持有当前 Muse 专用令牌的请求可在授权项目文件夹内运行开发工具与受控 PowerShell/cmd（默认限制于 Windows 写受限沙箱）。高风险与敏感目录操作仍需本机弹窗确认。开启后请重新复制接入说明发给 Muse。"),
                  action: t("开启命令执行"),
                  execute: async () => (await bridge.run(t("正在开启命令执行"), "set_exec_enabled", { enabled: true }, t("命令执行已开启，可重新复制接入说明发给 Muse"))).ok,
                });
              } else {
                void bridge.run(t("正在关闭命令执行"), "set_exec_enabled", { enabled: false }, t("命令执行已关闭"));
              }
            }}
          />
        </div>
        {execEnabled && execSessions.length > 0 ? (
          <div className="flex flex-wrap items-center justify-between gap-2 border-t pt-2">
            <span className="text-caption text-muted-foreground">
              {t("本会话已免打扰放行 {0} 个项目目录", execSessions.length)}
            </span>
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={() => void bridge.run(t("正在收回会话授权"), "revoke_exec_sessions", undefined, t("已收回所有项目会话执行授权"))}
            >
              {t("收回会话授权")}
            </Button>
          </div>
        ) : null}
      </div>
    </>}
  </section>;
}
