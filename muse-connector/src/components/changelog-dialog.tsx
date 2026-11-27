import { Fragment, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, RefreshCw, X } from "lucide-react";
import { useLanguage } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";

interface ChangelogResponse {
  markdown: string;
  source: "remote" | "bundled" | string;
  synced_at: string | null;
  remote_url: string;
}

interface BulletNode {
  text: string;
  children: string[];
}

interface SectionNode {
  title: string;
  items: BulletNode[];
}

interface ReleaseNode {
  version: string;
  date: string | null;
  sections: SectionNode[];
}

const fallbackRemoteUrls = {
  "zh-CN": "https://github.com/Aenvo/MuseLink/blob/main/CHANGELOG.md",
  en: "https://github.com/Aenvo/MuseLink/blob/main/CHANGELOG.en.md",
} as const;

function normalizeVersion(version: string) {
  return version.trim().replace(/^v/i, "");
}

function formatVersionHeading(version: string) {
  const clean = normalizeVersion(version);
  return clean ? `v${clean}` : version;
}

function formatIsoDate(value?: string | null) {
  if (!value) return null;
  const match = /^(\d{4}-\d{2}-\d{2})/.exec(value.trim());
  return match ? match[1] : null;
}

function isUnreleasedHeading(version: string) {
  const normalized = version.trim().toLowerCase();
  return normalized === "unreleased" || normalized === "未发布";
}

function parseSectionsFromLines(lines: string[]): SectionNode[] {
  const sections: SectionNode[] = [];
  let currentSection: SectionNode | null = null;

  for (const rawLine of lines) {
    const trimmed = rawLine.trim();
    if (!trimmed) continue;

    const sectionMatch = /^###\s+(.+)$/.exec(trimmed);
    if (sectionMatch) {
      currentSection = { title: sectionMatch[1].trim(), items: [] };
      sections.push(currentSection);
      continue;
    }

    const bulletMatch = /^(\s*)-\s+(.+)$/.exec(rawLine);
    if (bulletMatch) {
      if (!currentSection) {
        currentSection = { title: "", items: [] };
        sections.push(currentSection);
      }
      const indent = bulletMatch[1].length;
      const text = bulletMatch[2].trim();
      if (indent >= 2 && currentSection.items.length > 0) {
        currentSection.items[currentSection.items.length - 1].children.push(text);
      } else {
        currentSection.items.push({ text, children: [] });
      }
      continue;
    }

    if (!currentSection) {
      currentSection = { title: "", items: [] };
      sections.push(currentSection);
    }
    currentSection.items.push({ text: trimmed, children: [] });
  }

  return sections;
}

function parseChangelogMarkdown(markdown: string): ReleaseNode[] {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const releases: ReleaseNode[] = [];
  let currentRelease: ReleaseNode | null = null;
  let currentLines: string[] = [];
  let skipRelease = false;

  const flush = () => {
    if (currentRelease) {
      currentRelease.sections = parseSectionsFromLines(currentLines);
      releases.push(currentRelease);
    }
    currentRelease = null;
    currentLines = [];
  };

  for (const rawLine of lines) {
    const trimmed = rawLine.trim();
    const releaseMatch = /^##\s+\[([^\]]+)\](?:\s*[-–—]\s*(.+))?$/.exec(trimmed);
    if (releaseMatch) {
      flush();
      const version = releaseMatch[1].trim();
      const date = releaseMatch[2]?.trim() || null;
      if (isUnreleasedHeading(version)) {
        skipRelease = true;
        continue;
      }
      skipRelease = false;
      currentRelease = { version, date, sections: [] };
      continue;
    }
    if (!skipRelease && currentRelease) {
      currentLines.push(rawLine);
    }
  }
  flush();
  return releases;
}

function buildFallbackReleaseNode(
  version: string,
  date: string | null | undefined,
  notes: string | null | undefined,
): ReleaseNode {
  const formattedVersion = formatVersionHeading(version);
  const formattedDate = formatIsoDate(date);
  if (notes && notes.trim()) {
    const parsed = parseChangelogMarkdown(notes);
    if (parsed.length > 0) {
      return {
        version: formattedVersion,
        date: parsed[0].date ?? formattedDate,
        sections: parsed[0].sections,
      };
    }
    return {
      version: formattedVersion,
      date: formattedDate,
      sections: parseSectionsFromLines(notes.replace(/\r\n/g, "\n").split("\n")),
    };
  }
  return {
    version: formattedVersion,
    date: formattedDate,
    sections: [],
  };
}

function renderInline(text: string, onOpenLink: (url: string) => void): ReactNode[] {
  const nodes: ReactNode[] = [];
  const tokenRegex = /(\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = tokenRegex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(<Fragment key={key++}>{text.slice(lastIndex, match.index)}</Fragment>);
    }
    if (match[2] !== undefined) {
      nodes.push(<strong key={key++} className="font-medium text-foreground">{renderInline(match[2], onOpenLink)}</strong>);
    } else if (match[3] !== undefined) {
      nodes.push(
        <code key={key++} className="rounded bg-muted px-1 py-0.5 font-mono text-[0.78rem] text-foreground">
          {match[3]}
        </code>,
      );
    } else if (match[4] !== undefined && match[5] !== undefined) {
      const label = match[4];
      const href = match[5].trim();
      if (/^https?:\/\//i.test(href)) {
        nodes.push(
          <button
            key={key++}
            type="button"
            className="inline cursor-pointer underline underline-offset-4 hover:text-foreground"
            onClick={() => onOpenLink(href)}
          >
            {label}
          </button>,
        );
      } else {
        nodes.push(<span key={key++} className="text-foreground">{label}</span>);
      }
    }
    lastIndex = tokenRegex.lastIndex;
  }

  if (lastIndex < text.length) {
    nodes.push(<Fragment key={key++}>{text.slice(lastIndex)}</Fragment>);
  }
  return nodes;
}

export default function ChangelogDialog({
  open,
  mode,
  onOpenChange,
  currentVersion,
  latestVersion,
  releaseNotes,
  releaseDate,
  canInstallInApp,
  installing,
  onTriggerUpdate,
  onOpenReleases,
  onError,
}: {
  open: boolean;
  mode: "all" | "release";
  onOpenChange: (open: boolean) => void;
  currentVersion: string;
  latestVersion?: string | null;
  releaseNotes?: string | null;
  releaseDate?: string | null;
  canInstallInApp?: boolean;
  installing?: boolean;
  onTriggerUpdate?: () => void;
  onOpenReleases?: () => void;
  onError: (message: string) => void;
}) {
  const { language, t } = useLanguage();
  const [markdown, setMarkdown] = useState("");
  const [remoteUrl, setRemoteUrl] = useState<string>(fallbackRemoteUrls[language]);
  const [syncing, setSyncing] = useState(false);
  const [offlineFallback, setOfflineFallback] = useState(false);

  const syncRemote = useCallback(async (activeRef?: { cancelled: boolean }) => {
    if (!isTauri()) return;
    setSyncing(true);
    setOfflineFallback(false);
    try {
      const remote = await invoke<ChangelogResponse>("fetch_remote_changelog", { language });
      if (activeRef?.cancelled) return;
      setMarkdown(remote.markdown);
      setRemoteUrl(remote.remote_url);
      setOfflineFallback(false);
    } catch {
      if (activeRef?.cancelled) return;
      setOfflineFallback(true);
    } finally {
      if (!activeRef?.cancelled) setSyncing(false);
    }
  }, [language]);

  useEffect(() => {
    if (!open) return;
    const state = { cancelled: false };
    setRemoteUrl(fallbackRemoteUrls[language]);
    if (!isTauri()) return;

    void invoke<ChangelogResponse>("get_bundled_changelog", { language })
      .then(bundled => {
        if (state.cancelled) return;
        setMarkdown(bundled.markdown);
        setRemoteUrl(bundled.remote_url);
      })
      .catch(() => undefined);

    void syncRemote(state);
    return () => {
      state.cancelled = true;
    };
  }, [open, language, syncRemote]);

  const allReleases = useMemo(() => parseChangelogMarkdown(markdown), [markdown]);

  const visibleReleases = useMemo(() => {
    if (mode === "all") return allReleases;
    if (!latestVersion) return allReleases.slice(0, 1);
    const target = normalizeVersion(latestVersion);
    const matched = allReleases.find(r => normalizeVersion(r.version) === target);
    if (matched) return [matched];
    return [buildFallbackReleaseNode(latestVersion, releaseDate, releaseNotes)];
  }, [allReleases, latestVersion, mode, releaseDate, releaseNotes]);

  const openExternal = useCallback((url: string) => {
    void openUrl(url).catch(() => {
      onError(t("无法打开更新日志，请稍后重试。"));
    });
  }, [onError, t]);

  const showNewVersionBadge = mode === "all"
    && Boolean(latestVersion && normalizeVersion(latestVersion) !== normalizeVersion(currentVersion));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[82dvh] flex-col gap-4 overflow-hidden rounded-[14px] p-6 shadow-[0_12px_32px_rgba(0,0,0,0.55)] sm:max-w-[640px]"
      >
        <DialogHeader className="shrink-0 gap-1">
          <div className="flex items-center justify-between gap-3">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <DialogTitle className="text-lg leading-7 font-bold">
                {mode === "release" ? t("更新详情") : t("更新日志")}
              </DialogTitle>
              {showNewVersionBadge && latestVersion ? (
                <span className="inline-flex items-center justify-center rounded-full border-[0.8px] border-status-success px-2 py-0.5 text-xs leading-4 font-medium text-status-success">
                  {t("发现新版本 v{0}", normalizeVersion(latestVersion))}
                </span>
              ) : null}
              <button
                type="button"
                aria-label={t("在 GitHub 查看")}
                title={t("在 GitHub 查看")}
                className="inline-flex size-6 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                onClick={() => openExternal(remoteUrl)}
              >
                <ExternalLink className="size-4" aria-hidden="true" />
              </button>
              {syncing ? <Spinner className="size-3.5 text-muted-foreground" aria-hidden="true" /> : null}
            </div>
            <DialogClose
              aria-label={t("关闭")}
              className="inline-flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <X className="size-4" aria-hidden="true" />
            </DialogClose>
          </div>
          {!syncing && offlineFallback ? (
            <p className="flex items-center gap-2 text-caption text-muted-foreground">
              <span>{t("暂未连接 GitHub，当前展示本地缓存内容")}</span>
              <button
                type="button"
                className="inline-flex cursor-pointer items-center gap-1 text-foreground underline underline-offset-4 hover:opacity-85"
                onClick={() => void syncRemote()}
              >
                <RefreshCw className="size-3" aria-hidden="true" />
                {t("重试同步")}
              </button>
            </p>
          ) : null}
        </DialogHeader>

        <ScrollArea
          className="-mr-[22px] flex min-h-0 flex-1 flex-col overflow-hidden [&>[data-slot=scroll-area-viewport]]:h-auto [&>[data-slot=scroll-area-viewport]]:max-h-[calc(82dvh-148px)] [&>[data-slot=scroll-area-viewport]]:min-h-0 [&>[data-slot=scroll-area-viewport]]:flex-1 [&_[data-slot=scroll-area-scrollbar]]:w-[10px] [&_[data-slot=scroll-area-scrollbar]]:px-[2px] [&_[data-slot=scroll-area-thumb]]:bg-[#999999]/75 hover:[&_[data-slot=scroll-area-thumb]]:bg-[#999999]"
          type="always"
        >
          <div className="flex flex-col gap-6 pb-1 pr-[22px]">
            {visibleReleases.length === 0 ? (
              <p className="py-8 text-center text-muted-foreground">{t("暂无可展示的更新日志。")}</p>
            ) : (
              visibleReleases.map(release => (
                <section key={release.version} className="flex flex-col gap-3">
                  <div className="flex flex-col gap-2">
                    <div className="flex items-center justify-between gap-4">
                      <h3 className="text-base leading-6 font-bold text-foreground">{release.version}</h3>
                      {release.date ? (
                        <span className="shrink-0 text-caption leading-[21px] text-muted-foreground">{release.date}</span>
                      ) : null}
                    </div>
                    <div className="h-px w-full bg-border" />
                  </div>
                  {release.sections.length === 0 ? (
                    <p className="text-caption leading-[21px] text-muted-foreground">
                      {t("暂未获取到该版本的详细更新说明，可点击上方图标前往 GitHub 查看。")}
                    </p>
                  ) : (
                    release.sections.map((section, sectionIndex) => (
                      <div key={`${release.version}-${sectionIndex}`} className="flex flex-col gap-2">
                        {section.title ? (
                          <h4 className="text-caption leading-5 font-medium text-foreground">{section.title}</h4>
                        ) : null}
                        <div className="flex flex-col gap-1 pl-1 text-caption leading-[21px] text-muted-foreground">
                          {section.items.map((item, itemIndex) => (
                            <div key={itemIndex} className="flex flex-col gap-1">
                              <div className="flex items-start gap-2">
                                <span className="shrink-0 select-none text-muted-foreground" aria-hidden="true">•</span>
                                <p className="min-w-0 flex-1">{renderInline(item.text, openExternal)}</p>
                              </div>
                              {item.children.length > 0 ? (
                                <div className="flex flex-col gap-1 pl-[18px]">
                                  {item.children.map((child, childIndex) => (
                                    <div key={childIndex} className="flex items-start gap-2">
                                      <span className="shrink-0 select-none text-muted-foreground" aria-hidden="true">•</span>
                                      <p className="min-w-0 flex-1">{renderInline(child, openExternal)}</p>
                                    </div>
                                  ))}
                                </div>
                              ) : null}
                            </div>
                          ))}
                        </div>
                      </div>
                    ))
                  )}
                </section>
              ))
            )}
          </div>
        </ScrollArea>

        <DialogFooter className="shrink-0 flex-row justify-end">
          {mode === "release" ? (
            canInstallInApp ? (
              <Button
                variant="default"
                size="sm"
                disabled={installing}
                onClick={() => {
                  onOpenChange(false);
                  onTriggerUpdate?.();
                }}
              >
                {installing ? t("更新中…") : t("立即更新")}
              </Button>
            ) : (
              <Button
                variant="default"
                size="sm"
                onClick={() => {
                  onOpenChange(false);
                  onOpenReleases?.();
                }}
              >
                {t("前往下载")}
              </Button>
            )
          ) : (
            <Button variant="default" size="sm" onClick={() => onOpenChange(false)}>
              {t("关闭")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
