import { useLanguage } from "@/lib/i18n";
import { memo, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ScrollText, Copy, Check, Download, FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription } from "@/components/ui/empty";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { Notice } from "@/components/feedback";
import { formatTime, type BridgeModel, type LogRecord, type LogBatch } from "@/lib/use-bridge";
type LogKind = "audit" | "runtime";
type LogScope = "files" | "exec" | "audit" | "runtime";
type ExportScope = LogScope | "all_combined";
type ExportFormat = "jsonl" | "csv";
const fileEndpoints = new Set([
  "/api/list",
  "/api/read",
  "/api/write",
  "/api/mkdir",
  "/agent/v1/list",
  "/agent/v1/read",
  "/agent/v1/write",
  "/agent/v1/mkdir",
  "/agent/v1/search",
  "/agent/v1/locations/desktop",
]);
const kinds = ["audit", "runtime"] as const;
const logLimit = 1000;
const clock = { en: logClock("en"), "zh-CN": logClock("zh-CN") };

function logClock(language: string) {
  return new Intl.DateTimeFormat(language, { hour: "numeric", minute: "numeric", second: "numeric", hour12: false });
}

type ParsedLog = {
  record: LogRecord;
  time: string | null;
  timeMs: number;
  summary: string;
  code: string;
  searchText: string;
  object: Record<string, unknown> | null;
};
type LogEntries = Record<LogKind, ParsedLog[]>;

function parseLog(record: LogRecord): ParsedLog {
  const searchText = record.text.toLocaleLowerCase();
  try {
    const object = JSON.parse(record.text) as Record<string, unknown>;
    const time = typeof object.ts === "string" ? object.ts : null;
    const timeMs = time ? Date.parse(time) : NaN;
    return {
      record, time, timeMs, searchText, object,
      summary: [object.method, object.endpoint, object.root, object.path, object.message].filter(Boolean).join(" "),
      code: String(object.code ?? object.level ?? "—"),
    };
  } catch {
    return { record, time: null, timeMs: NaN, summary: record.text, code: "—", searchText, object: null };
  }
}

function useLogFeed(active: boolean) {
  const { t, diagnostic } = useLanguage();
  const [records, setRecords] = useState<LogEntries>({ audit: [], runtime: [] });
  const after = useRef<Record<LogKind, number | null>>({ audit: null, runtime: null });
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    let reading = false;
    let readAgain = false;
    let subscriptionFailed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let pending: Record<LogKind, LogRecord[]> = { audit: [], runtime: [] };

    const merge = (kind: LogKind, incoming: LogRecord[], replaceThrough?: number) => {
      if (incoming.length === 0 && replaceThrough === undefined) return;
      setRecords(previous => {
        const before = previous[kind];
        const cache = new Map(before.map(entry => [entry.record.seq, entry]));
        const entries = new Map(cache);
        if (replaceThrough !== undefined) {
          for (const seq of entries.keys()) {
            if (seq <= replaceThrough) entries.delete(seq);
          }
        }
        for (const record of incoming) {
          const cached = cache.get(record.seq);
          entries.set(record.seq, cached?.record.text === record.text && cached.record.persisted === record.persisted
            ? cached : parseLog(record));
        }
        const next = [...entries.values()].sort((a, b) => a.record.seq - b.record.seq).slice(-logLimit);
        return next.length === before.length && next.every((entry, index) => entry === before[index])
          ? previous : { ...previous, [kind]: next };
      });
    };

    const refresh = async () => {
      if (disposed) return;
      if (reading) { readAgain = true; return; }
      reading = true;
      try {
        const batches = await Promise.all(kinds.map(kind => invoke<LogBatch>("get_logs", { kind, after: after.current[kind] })));
        if (disposed) return;
        batches.forEach((batch, index) => {
          const kind = kinds[index];
          after.current[kind] = batch.last_seq;
          merge(kind, batch.entries, batch.gap ? batch.last_seq : undefined);
        });
        const errors = batches.flatMap(batch => batch.error ? [diagnostic(batch.error)] : []);
        if (subscriptionFailed) errors.push(t("日志事件订阅失败，已使用定时补齐。"));
        setError(errors.join(" · ") || null);
      } catch (e) {
        if (!disposed) setError(t("读取日志失败：{0}", diagnostic(String(e))));
      } finally {
        reading = false;
        if (readAgain && !disposed) { readAgain = false; void refresh(); }
      }
    };

    const subscriptions = Promise.allSettled([
      listen<LogRecord>("bridge-log", ({ payload }) => {
        if (disposed) return;
        const entries = pending[payload.kind];
        entries.push(payload);
        if (entries.length > logLimit) entries.shift();
        timer ??= setTimeout(() => {
          for (const kind of kinds) merge(kind, pending[kind]);
          pending = { audit: [], runtime: [] };
          timer = undefined;
        }, 100);
      }),
      listen("bridge-log-gap", () => { void refresh(); }),
    ]).then(results => {
      subscriptionFailed = results.some(result => result.status === "rejected");
      if (!disposed) void refresh();
      return results.flatMap(result => result.status === "fulfilled" ? [result.value] : []);
    });
    const fallback = setInterval(() => void refresh(), 15000);
    return () => {
      disposed = true;
      clearInterval(fallback);
      clearTimeout(timer);
      void subscriptions.then(stops => stops.forEach(stop => stop()));
    };
  }, [active, diagnostic, t]);
  return { records, error };
}

const LogTableRow = memo(function LogTableRow({ entry, show }: { entry: ParsedLog; show: (entry: ParsedLog, trigger: HTMLButtonElement) => void }) {
  const { language, t } = useLanguage();
  return <TableRow className="log-row"><TableCell className="text-sm tabular-nums">{entry.time && Number.isFinite(entry.timeMs) ? clock[language].format(entry.timeMs) : "—"}</TableCell><TableCell className="max-w-72 text-sm"><Tooltip><TooltipTrigger asChild><span className="block truncate">{entry.summary}</span></TooltipTrigger><TooltipContent className="max-w-sm break-all">{entry.summary}</TooltipContent></Tooltip></TableCell><TableCell><Badge variant="outline">{entry.code}</Badge></TableCell><TableCell><Button variant="ghost" size="sm" onClick={event => show(entry, event.currentTarget)} aria-label={t("查看日志 {0} 详情", entry.record.seq)}>{t("详情")}</Button></TableCell></TableRow>;
});
export default function Logs({ bridge, selected }: { bridge: BridgeModel; selected: boolean }) {
  const feed = useLogFeed(selected && bridge.windowVisible);
  return selected ? <LogView bridge={bridge} feed={feed} /> : null;
}

function filterLogRows(
  entries: ParsedLog[],
  scope: ExportScope,
  search: string,
  windowMinutes: string,
  resultFilter: string,
  nowMs: number,
) {
  return entries.filter(row => {
    if (scope === "files" && !fileEndpoints.has(String(row.object?.endpoint))) return false;
    if (scope === "exec" && !String(row.object?.endpoint ?? "").startsWith("/agent/v1/exec")) return false;
    if (search && !row.searchText.includes(search)) return false;
    if (windowMinutes !== "all" && (!row.time || row.timeMs < nowMs - Number(windowMinutes) * 60000)) return false;
    const failure = Number(row.code) >= 400 || row.code === "error" || row.code === "warning";
    return resultFilter === "all" || (resultFilter === "errors" ? failure : !failure);
  });
}

function csvCell(value: unknown) {
  const text = value === null || value === undefined ? "" : String(value);
  return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

function formatExportContent(rows: ParsedLog[], format: ExportFormat, includeKind: boolean) {
  if (format === "jsonl") {
    return rows.map(entry => {
      if (includeKind && entry.object) {
        return JSON.stringify({ kind: entry.record.kind, ...entry.object });
      }
      return entry.record.text;
    }).join("\n") + (rows.length ? "\n" : "");
  }
  const header = ["ts", "kind", "result", "method", "endpoint", "root", "path", "message", "bytes", "persisted"];
  const lines = [header.join(",")];
  for (const entry of rows) {
    const obj = entry.object;
    lines.push([
      entry.time ?? "",
      entry.record.kind,
      entry.code,
      obj?.method ?? "",
      obj?.endpoint ?? "",
      obj?.root ?? "",
      obj?.path ?? "",
      obj ? (obj.message ?? "") : entry.record.text,
      obj?.bytes ?? "",
      entry.record.persisted ? "true" : "false",
    ].map(csvCell).join(","));
  }
  return `\uFEFF${lines.join("\r\n")}\r\n`;
}

function defaultExportFileName(scope: ExportScope, format: ExportFormat) {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  const tag = scope === "all_combined" ? "all" : scope;
  return `muselink-logs-${tag}-${stamp}.${format}`;
}

function LogView({ bridge, feed }: { bridge: BridgeModel; feed: ReturnType<typeof useLogFeed> }) {
  const { language, t } = useLanguage();
  const [scope, setScope] = useState<LogScope>("files");
  const kind: LogKind = scope === "runtime" ? "runtime" : "audit";
  const [query, setQuery] = useState("");
  const search = useDeferredValue(query.toLocaleLowerCase());
  const [resultFilter, setResultFilter] = useState("all");
  const [windowMinutes, setWindowMinutes] = useState("all");
  const [frozen, setFrozen] = useState<ParsedLog[] | null>(null);
  const [detail, setDetail] = useState<ParsedLog | null>(null);
  const [copied, setCopied] = useState(false);
  const [limit, setLimit] = useState(200);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportScope, setExportScope] = useState<ExportScope>("files");
  const [exportResult, setExportResult] = useState("all");
  const [exportWindow, setExportWindow] = useState("all");
  const [exportQuery, setExportQuery] = useState("");
  const [exportFormat, setExportFormat] = useState<ExportFormat>("jsonl");
  const [exporting, setExporting] = useState(false);
  const detailTrigger = useRef<HTMLButtonElement | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const source = frozen ?? feed.records[kind];
  const rows = useMemo(
    () => filterLogRows(source, scope, search, windowMinutes, resultFilter, Date.now()).reverse(),
    [source, scope, search, windowMinutes, resultFilter],
  );
  const exportRows = useMemo(() => {
    if (!exportOpen) return [];
    const base = exportScope === "all_combined"
      ? [...feed.records.audit, ...feed.records.runtime].sort((a, b) => {
        const ta = Number.isFinite(a.timeMs) ? a.timeMs : 0;
        const tb = Number.isFinite(b.timeMs) ? b.timeMs : 0;
        return ta !== tb ? ta - tb : a.record.seq - b.record.seq;
      })
      : exportScope === "runtime"
        ? feed.records.runtime
        : feed.records.audit;
    return filterLogRows(base, exportScope, exportQuery.toLocaleLowerCase(), exportWindow, exportResult, Date.now());
  }, [exportOpen, exportScope, exportQuery, exportWindow, exportResult, feed.records.audit, feed.records.runtime]);
  const newCount = frozen ? feed.records[kind].filter(entry => entry.record.seq > (frozen[frozen.length - 1]?.record.seq ?? 0) && (scope === "files" ? fileEndpoints.has(String(entry.object?.endpoint)) : scope === "exec" ? String(entry.object?.endpoint ?? "").startsWith("/agent/v1/exec") : true)).length : 0;
  const show = useCallback((entry: ParsedLog, trigger: HTMLButtonElement) => { detailTrigger.current = trigger; setDetail(entry); }, []);
  const openExport = () => {
    setExportScope(scope);
    setExportResult(resultFilter);
    setExportWindow(windowMinutes);
    setExportQuery(query);
    setExportOpen(true);
  };
  const runExport = async () => {
    if (!exportRows.length || exporting) return;
    setExporting(true);
    try {
      const content = formatExportContent(exportRows, exportFormat, exportScope === "all_combined");
      const defaultName = defaultExportFileName(exportScope, exportFormat);
      const saved = await invoke<string | null>("export_logs", { defaultName, format: exportFormat, content });
      if (saved) {
        toast.success(t("已导出 {0} 条日志", exportRows.length));
        setExportOpen(false);
      }
    } catch (e) {
      bridge.setError(t("导出日志失败：{0}", String(e)));
    } finally {
      setExporting(false);
    }
  };
  const openLogsFolder = () => {
    void bridge.run(t("正在打开日志目录"), "open_logs_folder", { kind }, undefined);
  };
  return <div className="page-stack">
    {feed.error ? <Notice title={t("日志存储需要检查")} error>{feed.error}</Notice> : null}
    <Card><CardHeader><div className="flex flex-wrap items-center justify-between gap-2"><div className="flex flex-wrap items-center gap-3"><CardTitle>{t("日志")}</CardTitle><span className="text-caption text-muted-foreground" role="status">{frozen ? t("已暂停更新") : t("实时更新")} {t("· 匹配 {0} 条", rows.length)}</span></div><div className="flex flex-wrap items-center gap-2"><Button variant="outline" size="sm" onClick={openExport} disabled={!isTauri()}><Download data-icon="inline-start" />{t("导出日志")}</Button><Button variant="ghost" size="sm" onClick={openLogsFolder} disabled={!isTauri() || !!bridge.busy}><FolderOpen data-icon="inline-start" />{t("打开日志目录")}</Button><Button variant="outline" size="sm" onClick={() => setFrozen(value => value ? null : [...feed.records[kind]])}>{frozen ? t("继续更新") + (newCount > 0 ? ` (${newCount})` : "") : t("暂停实时更新")}</Button></div></div></CardHeader><CardContent className="flex flex-col gap-5">
      <FieldGroup className="grid grid-cols-2 gap-4 min-[600px]:grid-cols-[1.5fr_1fr_1fr]">
        <Field className="col-span-2 min-[600px]:col-span-1"><FieldLabel className="sr-only min-[600px]:not-sr-only" htmlFor="log-query">{t("检索")}</FieldLabel><Input ref={searchInput} id="log-query" name="log_query" autoComplete="off" spellCheck={false} value={query} onChange={e => setQuery(e.target.value)} placeholder={t("检索端点、目录、路径或消息")} /></Field>
        <Field><FieldLabel className="sr-only min-[600px]:not-sr-only" htmlFor="log-kind">{t("显示内容")}</FieldLabel><Select value={scope} onValueChange={value => { setScope(value as LogScope); setFrozen(null); setLimit(200); }}><SelectTrigger id="log-kind" className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="files">{t("文件操作")}</SelectItem><SelectItem value="exec">{t("命令执行")}</SelectItem><SelectItem value="audit">{t("全部访问（含健康探测）")}</SelectItem><SelectItem value="runtime">{t("连接器运行日志")}</SelectItem></SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel className="sr-only min-[600px]:not-sr-only" htmlFor="log-result">{t("结果")}</FieldLabel><Select value={resultFilter} onValueChange={setResultFilter}><SelectTrigger id="log-result" className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="all">{t("所有结果")}</SelectItem><SelectItem value="errors">{t("错误与警告")}</SelectItem><SelectItem value="success">{t("正常记录")}</SelectItem></SelectGroup></SelectContent></Select></Field>
      </FieldGroup>
      <details><summary className="cursor-pointer text-caption text-muted-foreground">{t("更多筛选与保留说明")}{windowMinutes !== "all" ? t(" · 最近 {0} 分钟", windowMinutes) : ""}</summary><FieldGroup className="mt-3"><Field><FieldLabel htmlFor="log-time">{t("时间范围")}</FieldLabel><Select value={windowMinutes} onValueChange={setWindowMinutes}><SelectTrigger id="log-time" className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="all">{t("全部缓存")}</SelectItem><SelectItem value="5">{t("最近 5 分钟")}</SelectItem><SelectItem value="15">{t("最近 15 分钟")}</SelectItem><SelectItem value="60">{t("最近 1 小时")}</SelectItem></SelectGroup></SelectContent></Select></Field></FieldGroup><p className="mt-3 text-caption text-muted-foreground">{t("每类最多缓存 1000 条；日志文件按 10 MiB 轮转，保留 3 份历史。健康探测保留在“全部访问”中。")}</p></details>
      {rows.length ? <><Table><TableHeader><TableRow><TableHead>{t("时间")}</TableHead><TableHead>{t("操作或消息")}</TableHead><TableHead>{t("结果")}</TableHead><TableHead><span className="sr-only">{t("详情操作")}</span></TableHead></TableRow></TableHeader><TableBody>{rows.slice(0, limit).map(entry => <LogTableRow key={entry.record.seq} entry={entry} show={show} />)}</TableBody></Table>{rows.length > limit ? <Button variant="outline" onClick={() => setLimit(value => value + 200)}>{t("再显示 200 条")}</Button> : null}</> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ScrollText /></EmptyMedia><EmptyTitle>{t("暂无匹配记录")}</EmptyTitle><EmptyDescription>{t("连接器收到请求或发生状态变化后会记录日志，也可以调整筛选条件。")}</EmptyDescription></EmptyHeader></Empty>}
    </CardContent></Card>
    <Dialog open={exportOpen} onOpenChange={open => { if (!exporting) setExportOpen(open); }}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("导出日志")}</DialogTitle>
          <DialogDescription>{t("选择要导出的日志范围与格式。导出内容按时间顺序排列，且已自动脱敏令牌。")}</DialogDescription>
        </DialogHeader>
        <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field className="sm:col-span-2">
            <FieldLabel htmlFor="export-scope">{t("日志范围")}</FieldLabel>
            <Select value={exportScope} onValueChange={value => setExportScope(value as ExportScope)}>
              <SelectTrigger id="export-scope" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="all_combined">{t("全部日志（访问与运行）")}</SelectItem>
                  <SelectItem value="files">{t("文件操作")}</SelectItem>
                  <SelectItem value="exec">{t("命令执行")}</SelectItem>
                  <SelectItem value="audit">{t("全部访问（含健康探测）")}</SelectItem>
                  <SelectItem value="runtime">{t("连接器运行日志")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="export-result">{t("结果")}</FieldLabel>
            <Select value={exportResult} onValueChange={setExportResult}>
              <SelectTrigger id="export-result" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="all">{t("所有结果")}</SelectItem>
                  <SelectItem value="errors">{t("错误与警告")}</SelectItem>
                  <SelectItem value="success">{t("正常记录")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="export-window">{t("时间范围")}</FieldLabel>
            <Select value={exportWindow} onValueChange={setExportWindow}>
              <SelectTrigger id="export-window" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="all">{t("全部缓存")}</SelectItem>
                  <SelectItem value="5">{t("最近 5 分钟")}</SelectItem>
                  <SelectItem value="15">{t("最近 15 分钟")}</SelectItem>
                  <SelectItem value="60">{t("最近 1 小时")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field className="sm:col-span-2">
            <FieldLabel htmlFor="export-query">{t("关键词（可选）")}</FieldLabel>
            <Input id="export-query" autoComplete="off" spellCheck={false} value={exportQuery} onChange={e => setExportQuery(e.target.value)} placeholder={t("检索端点、目录、路径或消息")} />
          </Field>
          <Field className="sm:col-span-2">
            <FieldLabel htmlFor="export-format">{t("导出格式")}</FieldLabel>
            <Select value={exportFormat} onValueChange={value => setExportFormat(value as ExportFormat)}>
              <SelectTrigger id="export-format" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="jsonl">{t("JSONL（保留原始字段，适合排查与反馈问题）")}</SelectItem>
                  <SelectItem value="csv">{t("CSV（表格格式，适合 Excel 直接查看）")}</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
        </FieldGroup>
        <p className="text-caption text-muted-foreground" role="status">
          {exportRows.length ? t("匹配 {0} 条日志，点击导出后选择保存位置。", exportRows.length) : t("当前筛选条件下没有可导出的日志。")}
        </p>
        <DialogFooter>
          <Button variant="outline" disabled={exporting} onClick={() => setExportOpen(false)}>{t("取消")}</Button>
          <Button disabled={exporting || exportRows.length === 0} onClick={() => void runExport()}>
            <Download data-icon="inline-start" />
            {exporting ? t("正在导出…") : t("导出 ({0})", exportRows.length)}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    <Dialog open={detail !== null} onOpenChange={open => { if (!open) { setDetail(null); setCopied(false); } }}><DialogContent onCloseAutoFocus={event => { event.preventDefault(); (detailTrigger.current?.isConnected ? detailTrigger.current : searchInput.current)?.focus(); }}><DialogHeader><DialogTitle>{t("日志详情")}</DialogTitle><DialogDescription>{detail?.time ? formatTime(detail.time, language) : t("尚无记录")} · {detail?.record.persisted ? t("已写入日志文件") : t("尚未写入文件，请检查存储")}</DialogDescription></DialogHeader><pre className="max-h-[55vh] overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-3 text-caption">{detail?.object ? JSON.stringify(detail.object, null, 2) : detail?.record.text}</pre><DialogFooter><Button variant="outline" onClick={async () => { try { await navigator.clipboard.writeText(detail?.record.text ?? ""); setCopied(true); } catch { bridge.setError(t("日志复制失败，请选择内容后手动复制。")); } }}>{copied ? <Check data-icon="inline-start" /> : <Copy data-icon="inline-start" />}{copied ? t("已复制") : t("复制此条")}</Button><Button onClick={() => setDetail(null)}>{t("关闭")}</Button></DialogFooter></DialogContent></Dialog>
  </div>;
}
