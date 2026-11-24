import { useLanguage } from "@/lib/i18n";
import FileAccessPanel from "@/components/file-access-panel";
import AgentConnection from "@/components/agent-connection";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import type { Confirmation } from "@/components/feedback";
import type { BridgeModel } from "@/lib/use-bridge";
import type { DeviceNameModel } from "@/lib/use-device-name";

export default function Roots({ bridge, deviceName, onEditDeviceName, guided, guiding, onAccessSaved, confirm }: {
  bridge: BridgeModel;
  deviceName: DeviceNameModel;
  onEditDeviceName: () => void;
  guided: boolean;
  guiding: boolean;
  onAccessSaved: () => void;
  confirm: (value: Confirmation) => void;
}) {
  const { t } = useLanguage();
  const snapshot = bridge.snapshot;
  if (!snapshot) return null;

  return <div className="page-stack">
    <Card>
      <CardHeader><CardTitle>{t("文件访问权限")}</CardTitle></CardHeader>
      <CardContent className="flex flex-col gap-6">
        <FileAccessPanel bridge={bridge} snapshot={snapshot} guided={guided} onSaved={onAccessSaved} />
        {!guided && !guiding ? <><Separator /><AgentConnection bridge={bridge} snapshot={snapshot} deviceName={deviceName} onEditDeviceName={onEditDeviceName} confirm={confirm} /></> : null}
      </CardContent>
    </Card>
  </div>;
}
