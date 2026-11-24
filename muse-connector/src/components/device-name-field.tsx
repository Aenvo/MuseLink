import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useLanguage } from "@/lib/i18n";
import type { DeviceNameModel } from "@/lib/use-device-name";

export default function DeviceNameField({ id, deviceName }: { id: string; deviceName: DeviceNameModel }) {
  const { t } = useLanguage();
  const { value, dirty, disabled, canSave, invalid, error, update, save } = deviceName;
  return <form noValidate onSubmit={event => { event.preventDefault(); void save(); }}>
    <Field className="gap-2" data-invalid={invalid}>
      <FieldLabel htmlFor={id}>{t("设备名称")}</FieldLabel>
      <div className="flex flex-wrap items-center gap-2">
        <Input id={id} name="device_name" className="min-w-28 flex-1" value={value} required disabled={disabled}
          placeholder={t("例如：笔记本、台式机、客厅电脑")} autoComplete="off" spellCheck={false}
          aria-invalid={invalid} aria-describedby={invalid ? `${id}-help ${id}-error` : `${id}-help`}
          onChange={event => update(event.target.value)} />
        <Button type="submit" size="sm" disabled={!canSave}>{t("保存名称")}</Button>
        {dirty ? <Button type="button" size="sm" variant="ghost" disabled={disabled} onClick={() => update(null)}>{t("取消")}</Button> : null}
      </div>
      <FieldDescription id={`${id}-help`}>{t("这个名字会写进发给 Muse 的接入说明，用来区分不同的电脑。")}</FieldDescription>
      {invalid ? <FieldError id={`${id}-error`}>{t("请输入 1–32 个字符，可用文字、0–9、空格、连字符和下划线，首尾不能有空格。")}</FieldError> : null}
      {error ? <FieldError>{error}</FieldError> : null}
    </Field>
  </form>;
}
