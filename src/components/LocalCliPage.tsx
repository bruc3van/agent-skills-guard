import { useEffect, useMemo, useRef, useState } from "react";
import {
  RefreshCw,
  Loader2,
  Search,
  SearchX,
  Download,
  Package,
  Trash2,
  FolderOpen,
} from "lucide-react";
import {
  useLocalCliTools,
  useRescanLocalCliTools,
  useCheckLocalCliUpdates,
  useUpdateLocalCliTool,
  useUninstallLocalCliTool,
} from "../hooks/useLocalCli";
import { managerLabel } from "../lib/local-cli";
import { useTranslation } from "react-i18next";
import { PageBusyNotice } from "./ui/PageBusyNotice";
import type { LocalCliTool } from "../types";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";
import { SkillUninstallConfirmDialog } from "./SkillUninstallConfirmDialog";
import { sanitizeTerminalText } from "../lib/terminal-log";

type ManagerTab = "all" | string;

const VISIBLE_MANAGERS = ["npm", "pnpm", "pip", "brew", "scoop", "choco", "unknown"];

export function LocalCliPage() {
  const { t } = useTranslation();
  const { data: tools = [], isLoading, refetch } = useLocalCliTools();
  const { mutate: rescanTools, isPending: isRescanning } = useRescanLocalCliTools();
  const { mutate: checkUpdates, isPending: isChecking } = useCheckLocalCliUpdates();
  const {
    mutate: updateTool,
    isPending: isUpdating,
    variables: updatingTool,
  } = useUpdateLocalCliTool();
  const {
    mutateAsync: uninstallTool,
    isPending: isUninstalling,
    variables: uninstallingTool,
  } = useUninstallLocalCliTool();
  const [search, setSearch] = useState("");
  const [activeTab, setActiveTab] = useState<ManagerTab>("all");
  const [showUpdatesOnly, setShowUpdatesOnly] = useState(false);
  const [isHeaderCollapsed, setIsHeaderCollapsed] = useState(false);
  const listContainerRef = useRef<HTMLDivElement | null>(null);
  const [pendingUninstall, setPendingUninstall] = useState<LocalCliTool | null>(null);

  // Description cache: toolId -> description
  const [descriptionMap, setDescriptionMap] = useState<Record<string, string>>({});
  const [isFetchingDesc, setIsFetchingDesc] = useState(false);
  const [fetchProgress, setFetchProgress] = useState<{
    current: string;
    done: number;
    total: number;
  } | null>(null);
  const [descriptionRetryToken, setDescriptionRetryToken] = useState(0);
  const attemptedDescriptionIdsRef = useRef<Set<string>>(new Set());

  // Lazily fetch descriptions for tools missing one — one by one with progress
  useEffect(() => {
    if (isLoading || tools.length === 0) return;
    const missing = tools.filter(
      (t) =>
        !t.description && !descriptionMap[t.id] && !attemptedDescriptionIdsRef.current.has(t.id)
    );
    if (missing.length === 0) return;

    for (const tool of missing) {
      attemptedDescriptionIdsRef.current.add(tool.id);
    }

    let cancelled = false;
    setIsFetchingDesc(true);

    const total = missing.length;

    const fetchNext = async (index: number) => {
      if (cancelled) return;
      if (index >= missing.length) {
        setIsFetchingDesc(false);
        setFetchProgress(null);
        void refetch();
        return;
      }
      const tool = missing[index];
      setFetchProgress({ current: tool.id, done: index, total });
      try {
        const results = await api.fetchLocalCliDescriptions([tool.detected_path]);
        if (results.length > 0) {
          const [, desc] = results[0];
          setDescriptionMap((prev) => ({ ...prev, [tool.id]: desc }));
        }
      } catch {
        // skip failed tool
      }
      void fetchNext(index + 1);
    };

    void fetchNext(0);
    return () => {
      cancelled = true;
    };
  }, [tools, isLoading, refetch, descriptionRetryToken]);

  const getToolDescription = (tool: LocalCliTool): string | undefined => {
    const raw = tool.description || descriptionMap[tool.id];
    return raw ? sanitizeTerminalText(raw) : undefined;
  };

  const updateCount = tools.filter((tool) => tool.update_available).length;

  const tabCounts = useMemo(() => {
    const counts: Record<string, number> = { all: tools.length };
    for (const tool of tools) {
      counts[tool.manager] = (counts[tool.manager] || 0) + 1;
    }
    return counts;
  }, [tools]);

  const activeManagers = useMemo(() => {
    const managers = new Set(tools.map((t) => t.manager));
    return VISIBLE_MANAGERS.filter((m) => managers.has(m));
  }, [tools]);

  const filtered = useMemo(() => {
    let items = tools;

    if (activeTab !== "all") {
      items = items.filter((t) => t.manager === activeTab);
    }

    if (showUpdatesOnly) {
      items = items.filter((t) => t.update_available);
    }

    const q = search.trim().toLowerCase();
    if (q) {
      items = items.filter(
        (t) =>
          t.id.toLowerCase().includes(q) ||
          t.detected_path.toLowerCase().includes(q) ||
          (t.package_name && t.package_name.toLowerCase().includes(q)) ||
          (getToolDescription(t) && getToolDescription(t)!.toLowerCase().includes(q))
      );
    }

    return [...items].sort((a, b) => {
      const updateDelta = Number(b.update_available) - Number(a.update_available);
      if (updateDelta !== 0) return updateDelta;
      if (q) {
        const aRank = a.id.toLowerCase().includes(q) ? 0 : 1;
        const bRank = b.id.toLowerCase().includes(q) ? 0 : 1;
        if (aRank !== bRank) return aRank - bRank;
      }
      return a.id.localeCompare(b.id);
    });
  }, [tools, activeTab, showUpdatesOnly, search, descriptionMap]);

  const busyMessage = useMemo(() => {
    if (isChecking) return t("localCli.checking");
    if (isUpdating && updatingTool) {
      return t("localCli.busy.updating", { name: updatingTool.id });
    }
    if (isUninstalling && uninstallingTool) {
      return t("localCli.busy.uninstalling", { name: uninstallingTool.id });
    }
    if (fetchProgress) {
      return t("localCli.busy.fetchingDesc", {
        name: fetchProgress.current,
        done: fetchProgress.done + 1,
        total: fetchProgress.total,
      });
    }
    return null;
  }, [isChecking, isUpdating, isUninstalling, fetchProgress, t, updatingTool, uninstallingTool]);

  const handleCheckUpdates = () => {
    checkUpdates(undefined, {
      onSuccess: () => {
        void refetch();
      },
    });
  };

  const handleRescan = () => {
    setDescriptionMap({});
    attemptedDescriptionIdsRef.current.clear();
    setFetchProgress(null);
    setIsFetchingDesc(false);
    setDescriptionRetryToken((value) => value + 1);
    rescanTools();
  };

  const handleUpdateTool = (tool: LocalCliTool) => {
    updateTool(tool);
  };

  const handleOpenFolder = async (tool: LocalCliTool) => {
    try {
      await api.openLocalCliFolder(tool.detected_path);
      appToast.success(t("localCli.folder.opened"), { duration: 3000 });
    } catch (error: any) {
      appToast.error(
        t("localCli.folder.openFailed", {
          error: error?.message || String(error),
        }),
        { duration: 5000 }
      );
    }
  };

  const handleConfirmUninstall = async () => {
    if (!pendingUninstall || isUninstalling) return;
    const tool = pendingUninstall;
    await uninstallTool(tool);
    setPendingUninstall(null);
  };

  const handleFocusUpdates = (tab?: string) => {
    setActiveTab(tab ?? "all");
    setShowUpdatesOnly(true);
    setSearch("");
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-shrink-0 border-b border-border/50">
        <div className="px-8 pt-8 pb-4" style={{ animation: "fadeIn 0.4s ease-out" }}>
          <div className="max-w-6xl mx-auto">
            <div
              className={`overflow-hidden transition-all duration-200 ${
                isHeaderCollapsed ? "max-h-0 opacity-0" : "max-h-24 opacity-100"
              }`}
            >
              <div className="flex items-center justify-between gap-4 mb-4">
                <h1 className="text-headline text-foreground">{t("localCli.title")}</h1>
                <div className="flex items-center gap-2">
                  <button
                    onClick={handleRescan}
                    disabled={isLoading || isRescanning}
                    className="apple-button-secondary h-10 px-4 flex items-center gap-2 disabled:opacity-50 text-sm"
                  >
                    {isLoading || isRescanning ? (
                      <Loader2 className="w-4 h-4 animate-spin" />
                    ) : (
                      <RefreshCw className="w-4 h-4" />
                    )}
                    {t("localCli.rescan")}
                  </button>
                  <button
                    onClick={handleCheckUpdates}
                    disabled={isChecking}
                    className="apple-button-primary h-10 px-5 flex items-center gap-2 disabled:opacity-50"
                  >
                    {isChecking ? (
                      <>
                        <Loader2 className="w-4 h-4 animate-spin" />
                        {t("localCli.checking")}
                      </>
                    ) : (
                      <>
                        <RefreshCw className="w-4 h-4" />
                        {t("localCli.checkUpdates")}
                      </>
                    )}
                  </button>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-2 mb-4 flex-wrap">
              <ManagerTabButton
                active={activeTab === "all"}
                onClick={() => {
                  setActiveTab("all");
                  setSearch("");
                }}
                label={t("localCli.tabs.all", { count: tabCounts.all })}
              />
              {activeManagers.map((manager) => (
                <ManagerTabButton
                  key={manager}
                  active={activeTab === manager}
                  onClick={() => {
                    setActiveTab(manager);
                    setSearch("");
                  }}
                  label={t(`localCli.tabs.${manager}`, {
                    count: tabCounts[manager] || 0,
                  })}
                />
              ))}
            </div>

            <div className="flex gap-3 items-center flex-wrap">
              <div className="relative flex-1 min-w-[300px]">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <input
                  type="text"
                  placeholder={t("localCli.search")}
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  className="apple-input w-full h-10 pl-11 pr-4"
                />
              </div>
            </div>

            {updateCount > 0 && (
              <div className="mt-4 flex flex-wrap items-center gap-2 rounded-2xl border border-primary/15 bg-primary/5 px-4 py-3">
                <span className="text-sm font-medium text-foreground">
                  {t("localCli.updatesFocus.title", { count: updateCount })}
                </span>
                <button
                  type="button"
                  onClick={() => handleFocusUpdates("all")}
                  className={`h-8 rounded-full px-3 text-xs transition-colors ${
                    activeTab === "all" && showUpdatesOnly
                      ? "bg-primary text-primary-foreground"
                      : "bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("localCli.tabs.all", { count: updateCount })}
                </button>
                {activeManagers
                  .filter((m) => tools.some((t) => t.manager === m && t.update_available))
                  .map((manager) => {
                    const count = tools.filter(
                      (t) => t.manager === manager && t.update_available
                    ).length;
                    return (
                      <button
                        key={manager}
                        type="button"
                        onClick={() => handleFocusUpdates(manager)}
                        className={`h-8 rounded-full px-3 text-xs transition-colors ${
                          activeTab === manager && showUpdatesOnly
                            ? "bg-primary text-primary-foreground"
                            : "bg-card text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        {managerLabel(manager)} ({count})
                      </button>
                    );
                  })}
                <button
                  type="button"
                  onClick={() => setShowUpdatesOnly((v) => !v)}
                  className="ml-auto h-8 rounded-full border border-border bg-card px-3 text-xs text-muted-foreground transition-colors hover:text-foreground"
                >
                  {showUpdatesOnly
                    ? t("localCli.updatesFocus.showAll")
                    : t("localCli.updatesFocus.showOnly")}
                </button>
              </div>
            )}

            {busyMessage && (
              <div className="mt-4">
                <PageBusyNotice message={busyMessage} />
              </div>
            )}
          </div>
        </div>
      </div>

      <div
        ref={listContainerRef}
        className="flex-1 overflow-y-auto overscroll-contain px-8 pb-8"
        onScroll={(e) => {
          const top = (e.currentTarget as HTMLDivElement).scrollTop;
          setIsHeaderCollapsed(top > 8);
        }}
      >
        <div className={`max-w-6xl mx-auto ${isHeaderCollapsed ? "pt-4" : "pt-6"}`}>
          {isLoading ? (
            <div className="flex flex-col items-center justify-center py-20">
              <Loader2 className="w-10 h-10 text-blue-500 animate-spin mb-4" />
              <p className="text-sm text-muted-foreground">{t("localCli.loading")}</p>
            </div>
          ) : filtered.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 items-start">
              {filtered.map((tool) => (
                <CliToolCard
                  key={tool.detected_path}
                  tool={tool}
                  description={getToolDescription(tool)}
                  isFetchingDesc={isFetchingDesc && !tool.description && !descriptionMap[tool.id]}
                  onUpdate={handleUpdateTool}
                  isUpdating={isUpdating && updatingTool?.detected_path === tool.detected_path}
                  onOpenFolder={handleOpenFolder}
                  onRequestUninstall={setPendingUninstall}
                  isUninstalling={
                    isUninstalling && uninstallingTool?.detected_path === tool.detected_path
                  }
                  isAnyOperationPending={isUpdating || isChecking || isUninstalling}
                />
              ))}
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-20 apple-card">
              <div className="w-20 h-20 rounded-full bg-secondary flex items-center justify-center mb-5">
                {search || activeTab !== "all" ? (
                  <SearchX className="w-10 h-10 text-muted-foreground" />
                ) : (
                  <Package className="w-10 h-10 text-muted-foreground" />
                )}
              </div>
              <p className="text-sm text-muted-foreground">
                {search || activeTab !== "all"
                  ? t("localCli.empty.noResults", { query: search })
                  : showUpdatesOnly
                    ? t("localCli.empty.noUpdates")
                    : t("localCli.empty.all")}
              </p>
              {(search || activeTab !== "all") && (
                <button
                  onClick={() => {
                    setSearch("");
                    setActiveTab("all");
                    setShowUpdatesOnly(false);
                  }}
                  className="mt-5 apple-button-secondary"
                >
                  {t("localCli.empty.clearFilters")}
                </button>
              )}
              {!search && activeTab === "all" && showUpdatesOnly && (
                <button
                  onClick={() => setShowUpdatesOnly(false)}
                  className="mt-5 apple-button-secondary"
                >
                  {t("localCli.updatesFocus.showAll")}
                </button>
              )}
            </div>
          )}
        </div>
      </div>

      <SkillUninstallConfirmDialog
        open={pendingUninstall !== null}
        skillName={pendingUninstall?.id ?? ""}
        operationCount={1}
        pathCount={1}
        isConfirming={
          pendingUninstall
            ? isUninstalling && uninstallingTool?.detected_path === pendingUninstall.detected_path
            : false
        }
        labels={{
          title: t("localCli.uninstallDialog.title"),
          description: t("localCli.uninstallDialog.description", {
            name: pendingUninstall?.id ?? "",
          }),
          impact: t("localCli.uninstallDialog.impact", {
            manager: pendingUninstall ? managerLabel(pendingUninstall.manager) : "",
            package: pendingUninstall?.package_name ?? pendingUninstall?.id ?? "",
          }),
          cancel: t("localCli.uninstallDialog.cancel"),
          confirm: t("localCli.uninstallDialog.confirm"),
          confirming: t("localCli.uninstallDialog.confirming"),
        }}
        onCancel={() => {
          if (!isUninstalling) setPendingUninstall(null);
        }}
        onConfirm={() => {
          void handleConfirmUninstall();
        }}
      />
    </div>
  );
}

function ManagerTabButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`h-9 px-4 rounded-lg text-sm transition-colors border ${
        active
          ? "bg-primary text-primary-foreground border-primary"
          : "bg-card text-muted-foreground border-border hover:text-foreground hover:border-primary/40"
      }`}
    >
      {label}
    </button>
  );
}

function formatLastChecked(value: string | undefined, fallback: string): string {
  if (!value) return fallback;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return fallback;

  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function CliToolCard({
  tool,
  description,
  isFetchingDesc,
  onUpdate,
  onOpenFolder,
  onRequestUninstall,
  isUpdating,
  isUninstalling,
  isAnyOperationPending,
}: {
  tool: LocalCliTool;
  description?: string;
  isFetchingDesc: boolean;
  onUpdate: (tool: LocalCliTool) => void;
  onOpenFolder: (tool: LocalCliTool) => void;
  onRequestUninstall: (tool: LocalCliTool) => void;
  isUpdating: boolean;
  isUninstalling: boolean;
  isAnyOperationPending: boolean;
}) {
  const { t } = useTranslation();
  const hasUpdate = tool.update_available;
  const packageName = tool.package_name && tool.package_name !== tool.id ? tool.package_name : null;
  const lastChecked = formatLastChecked(tool.last_checked, t("localCli.card.notChecked"));

  return (
    <div className="apple-card p-4 group flex flex-col relative">
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2.5 mb-1 flex-wrap">
            <h3 className="font-semibold text-foreground font-mono text-sm truncate">{tool.id}</h3>
            <span className="text-[10px] uppercase tracking-normal bg-secondary text-muted-foreground px-1.5 py-0.5 rounded-md font-medium">
              {managerLabel(tool.manager)}
            </span>
            {hasUpdate && (
              <span className="text-[10px] bg-amber-500/15 text-amber-600 border border-amber-500/40 px-2 py-0.5 rounded-full font-medium">
                {t("localCli.update")}
              </span>
            )}
            {tool.update_status === "success" && (
              <span className="text-[10px] bg-emerald-500/15 text-emerald-600 border border-emerald-500/40 px-2 py-0.5 rounded-full font-medium">
                {t("localCli.updated")}
              </span>
            )}
          </div>
          {packageName && (
            <div className="text-xs text-muted-foreground font-mono truncate">{packageName}</div>
          )}
          {tool.bundled_tool_ids && tool.bundled_tool_ids.length > 0 && (
            <div
              className="text-[11px] text-muted-foreground/60 mt-0.5"
              title={tool.bundled_tool_ids.join(", ")}
            >
              +{tool.bundled_tool_ids.length} {t("localCli.card.bundledTools")}
            </div>
          )}
        </div>

        <div className="flex gap-2 shrink-0">
          {hasUpdate && (
            <button
              onClick={() => onUpdate(tool)}
              disabled={isAnyOperationPending}
              className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5"
            >
              {isUpdating ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  {t("localCli.card.updating")}
                </>
              ) : (
                <>
                  <Download className="w-3.5 h-3.5" />
                  {t("localCli.update")}
                </>
              )}
            </button>
          )}
          <button
            onClick={() => onRequestUninstall(tool)}
            disabled={isAnyOperationPending}
            aria-label={`${t("localCli.uninstall")}: ${tool.id}`}
            title={`${t("localCli.uninstall")}: ${tool.id}`}
            className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5"
          >
            {isUninstalling ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("localCli.card.uninstalling")}
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5" />
                {t("localCli.uninstall")}
              </>
            )}
          </button>
        </div>
      </div>

      {/* Description */}
      {description ? (
        <p
          title={description}
          className="text-sm text-muted-foreground leading-5 mb-3 overflow-hidden [display:-webkit-box] [-webkit-line-clamp:2] [-webkit-box-orient:vertical]"
        >
          {description}
        </p>
      ) : isFetchingDesc ? (
        <div className="flex items-center gap-2 mb-3 text-xs text-muted-foreground/60">
          <Loader2 className="w-3 h-3 animate-spin" />
          <span>...</span>
        </div>
      ) : null}

      <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 mb-3 text-xs">
        <div className="min-w-0">
          <span className="text-muted-foreground">{t("localCli.card.version")}</span>{" "}
          <span className="font-mono font-medium text-foreground">
            {tool.current_version ?? t("localCli.card.noVersion")}
          </span>
        </div>
        {hasUpdate && tool.latest_version && (
          <div className="min-w-0">
            <span className="text-muted-foreground">{t("localCli.card.latest")}</span>{" "}
            <span className="font-mono font-medium text-amber-600">v{tool.latest_version}</span>
          </div>
        )}
        <div className="min-w-0 col-span-2">
          <span className="text-muted-foreground">{t("localCli.card.lastChecked")}</span>{" "}
          <span className="font-mono text-foreground/80">{lastChecked}</span>
        </div>
      </div>

      <div className="pt-3 border-t border-border/60">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => onOpenFolder(tool)}
            aria-label={`${t("localCli.card.openFolder")}: ${tool.detected_path}`}
            title={`${t("localCli.card.openFolder")}: ${tool.detected_path}`}
            className="text-blue-500 hover:text-blue-600 transition-colors"
          >
            <FolderOpen className="w-4 h-4" />
          </button>
          <p
            title={tool.detected_path}
            className="text-xs text-muted-foreground/60 font-mono truncate"
          >
            {tool.detected_path}
          </p>
        </div>
      </div>
    </div>
  );
}
