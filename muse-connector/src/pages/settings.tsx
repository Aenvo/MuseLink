import { useLanguage } from "@/lib/i18n";
import { FolderOpen } from "lucide-react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel, FieldContent, FieldDescription } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { Language } from "@/lib/i18n";
import type { BridgeModel } from "@/lib/use-bridge";

export default function Settings({ bridge }: { bridge: BridgeModel }) {
  const { t } = useLanguage();
  const { snapshot: s, busy, run } = bridge;
  if (!s) return null;
  const development = s.profile === "development";
  return <div className="page-stack">
    <Card>
      <CardContent>
        <Field orientation="horizontal">
          <FieldLabel htmlFor="display-language">{t("界面语言")}</FieldLabel>
          <Select value={s.desktop.language ?? "zh-CN"} disabled={!!busy} onValueChange={(language: Language) => void run(t("正在切换界面语言"), "set_language", { language })}>
            <SelectTrigger id="display-language"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="zh-CN"><span lang="zh-CN">简体中文</span></SelectItem>
              <SelectItem value="en"><span lang="en">English</span></SelectItem>
            </SelectContent>
          </Select>
        </Field>
      </CardContent>
    </Card>
    <Card><CardHeader><CardTitle>{t("启动与窗口")}</CardTitle><CardDescription>{t("已暂停的连接不会因自启而开启。")}</CardDescription></CardHeader><CardContent><FieldGroup>
      <Field orientation="horizontal"><FieldContent><FieldLabel htmlFor="autostart">{t("登录 Windows 时启动")}</FieldLabel>{development || s.autostart === null ? <FieldDescription>{development ? t("开发模式不修改正式自启项。") : t("无法读取系统注册状态，请稍后重试。")}</FieldDescription> : null}</FieldContent><Switch id="autostart" checked={s.autostart === true} disabled={!!busy || development || s.autostart === null} onCheckedChange={enable => void run(t("正在设置登录自启"), "set_autostart", { enable }, t("登录自启设置已更新"))} /></Field>
      <Field orientation="horizontal"><FieldContent><FieldLabel htmlFor="start-minimized">{t("启动时折叠到托盘")}</FieldLabel><FieldDescription>{t("关闭主窗口也会折叠到托盘，可从托盘恢复。")}</FieldDescription></FieldContent><Switch id="start-minimized" checked={s.desktop.start_minimized} disabled={!!busy} onCheckedChange={startMinimized => void run(t("正在保存启动偏好"), "save_preferences", { startMinimized }, t("启动偏好已保存"))} /></Field>
    </FieldGroup></CardContent></Card>
    <Card><CardHeader><CardTitle>{t("本机数据")}</CardTitle><CardDescription>{t("配置、文件令牌和日志保存在此处。")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-5">
      <p className="select-text break-all text-sm text-muted-foreground">{s.home}</p><div><Button variant="outline" disabled={!!busy} onClick={() => void run(t("正在打开数据目录"), "open_local_folder")}><FolderOpen data-icon="inline-start" />{t("打开数据目录")}</Button></div>
    </CardContent></Card>
  </div>;
}
