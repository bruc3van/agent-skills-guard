import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useInstalledSkills, useUninstallSkill, useUninstallSkillPath } from "../hooks/useSkills";
import {
  useClaudeMarketplaces,
  usePlugins,
  useRemoveMarketplace,
  useUninstallPlugin,
} from "../hooks/usePlugins";
import { Plugin, Skill, SkillPluginUpgradeCandidate } from "../types";
import { SecurityReport } from "../types/security";
import {
  Trash2,
  Loader2,
  FolderOpen,
  Package,
  Search,
  SearchX,
  Download,
  Plug,
  Lightbulb,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { formatRepositoryTag } from "../lib/utils";
import { CyberSelect, type CyberSelectOption } from "./ui/CyberSelect";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";
import { normalizeInstalledSkills } from "@/lib/installed-skills";
import { PageBusyNotice } from "./ui/PageBusyNotice";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
} from "./ui/alert-dialog";
import {
  SkillSecurityDialog,
  SkillSecurityDialogConfirmButton,
} from "./ui/SkillSecurityDialog";

const AVAILABLE_UPDATES_KEY = "available_updates";
const AVAILABLE_PLUGIN_UPDATES_KEY = "available_plugin_updates";
const AVAILABLE_MARKETPLACE_UPDATES_KEY = "available_marketplace_updates";

type InstalledMarketplace = {
  name: string;
  repoUrl: string;
  plugins: Plugin[];
  installedCount: number;
  totalCount: number;
};

type InstalledEntry =
  | { kind: "skill"; item: Skill }
  | { kind: "plugin"; item: Plugin }
  | { kind: "marketplace"; item: InstalledMarketplace };

type InstalledOpsStatus = {
  uninstallingSkillId: string | null;
  uninstallingPluginId: string | null;
  pendingMarketplaceRemove: {
    marketplaceName: string;
    marketplaceRepo: string;
    installedPluginNames: string[];
  } | null;
  removingMarketplaceName: string | null;
};

const MARKETPLACE_OWNER_REGEX = /github\.com\/([^/]+)/;

const getMarketplaceOwner = (repoUrl: string) => {
  if (!repoUrl) return "unknown";
  if (repoUrl === "local") return "local";
  const match = repoUrl.match(MARKETPLACE_OWNER_REGEX);
  return match ? match[1] : "unknown";
};

export function InstalledSkillsPage() {
  const { t, i18n } = useTranslation();
  const { data: installedSkills, isLoading: isSkillsLoading } = useInstalledSkills();
  const { data: allPlugins = [], isLoading: isPluginsLoading } = usePlugins();
  const { data: claudeMarketplaces = [], isLoading: isMarketplacesLoading } = useClaudeMarketplaces();
  const { data: featuredMarketplaces } = useQuery({
    queryKey: ["featured-marketplaces"],
    queryFn: api.getFeaturedMarketplaces,
    staleTime: 5 * 60 * 1000,
    retry: false,
  });
  const uninstallMutation = useUninstallSkill();
  const uninstallPathMutation = useUninstallSkillPath();
  const uninstallPluginMutation = useUninstallPlugin();
  const removeMarketplaceMutation = useRemoveMarketplace();
  const queryClient = useQueryClient();
  const listContainerRef = useRef<HTMLDivElement | null>(null);
  const [isHeaderCollapsed, setIsHeaderCollapsed] = useState(false);

  const installedOpsQueryKey = ["installed", "ops-status"];
  const defaultInstalledOps: InstalledOpsStatus = {
    uninstallingSkillId: null,
    uninstallingPluginId: null,
    pendingMarketplaceRemove: null,
    removingMarketplaceName: null,
  };
  const { data: installedOps = defaultInstalledOps } = useQuery<InstalledOpsStatus>({
    queryKey: installedOpsQueryKey,
    queryFn: () => defaultInstalledOps,
    initialData: defaultInstalledOps,
    staleTime: Infinity,
    gcTime: Infinity,
  });
  const setInstalledOps = (updater: (prev: InstalledOpsStatus) => InstalledOpsStatus) => {
    queryClient.setQueryData(installedOpsQueryKey, (prev?: InstalledOpsStatus) =>
      updater(prev ?? defaultInstalledOps)
    );
  };

  const [activeTab, setActiveTab] = useState<"all" | "skills" | "plugins" | "marketplaces">("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedRepository, setSelectedRepository] = useState("all");
  const [showUpdatesOnly, setShowUpdatesOnly] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const {
    uninstallingSkillId,
    uninstallingPluginId,
    pendingMarketplaceRemove,
    removingMarketplaceName,
  } = installedOps;

  const [availableUpdates, setAvailableUpdates] = useState<Map<string, string>>(() => {
    try {
      const stored = localStorage.getItem(AVAILABLE_UPDATES_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        return new Map(Object.entries(parsed));
      }
    } catch (error) {
      console.error("[ERROR] 恢复更新状态失败:", error);
    }
    return new Map();
  });
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);
  const [preparingUpdateSkillId, setPreparingUpdateSkillId] = useState<string | null>(null);
  const [confirmingUpdateSkillId, setConfirmingUpdateSkillId] = useState<string | null>(null);
  const [pendingUpdate, setPendingUpdate] = useState<{
    skill: Skill;
    report: SecurityReport;
    conflicts: string[];
  } | null>(null);
  const [pendingSkillPluginUpgrade, setPendingSkillPluginUpgrade] =
    useState<SkillPluginUpgradeCandidate | null>(null);

  const [availablePluginUpdates, setAvailablePluginUpdates] = useState<Map<string, string>>(() => {
    try {
      const stored = localStorage.getItem(AVAILABLE_PLUGIN_UPDATES_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        return new Map(Object.entries(parsed));
      }
    } catch (error) {
      console.error("[ERROR] 恢复插件更新状态失败:", error);
    }
    return new Map();
  });
  const [isCheckingPluginUpdates, setIsCheckingPluginUpdates] = useState(false);
  const [updatingPluginId, setUpdatingPluginId] = useState<string | null>(null);

  const [availableMarketplaceUpdates, setAvailableMarketplaceUpdates] = useState<
    Map<string, string>
  >(() => {
    try {
      const stored = localStorage.getItem(AVAILABLE_MARKETPLACE_UPDATES_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        return new Map(Object.entries(parsed));
      }
    } catch (error) {
      console.error("[ERROR] 恢复 Marketplace 更新状态失败:", error);
    }
    return new Map();
  });
  const [isCheckingMarketplaceUpdates, setIsCheckingMarketplaceUpdates] = useState(false);
  const [updatingMarketplaceName, setUpdatingMarketplaceName] = useState<string | null>(null);
  const [isCheckingAllUpdates, setIsCheckingAllUpdates] = useState(false);

  const getLocalizedText = (text?: { en: string; zh: string }) => {
    if (!text) return "";
    return i18n.language === "zh" ? text.zh : text.en;
  };

  const marketplaceDescriptions = useMemo(() => {
    const map = new Map<string, string>();
    if (!featuredMarketplaces?.marketplace) return map;
    featuredMarketplaces.marketplace.forEach((category) => {
      category.marketplaces.forEach((marketplace) => {
        const description = getLocalizedText(marketplace.description);
        if (description) {
          map.set(marketplace.marketplace_name, description);
        }
      });
    });
    return map;
  }, [featuredMarketplaces, i18n.language]);

  useEffect(() => {
    try {
      if (availableUpdates.size > 0) {
        const obj = Object.fromEntries(availableUpdates);
        localStorage.setItem(AVAILABLE_UPDATES_KEY, JSON.stringify(obj));
      } else {
        localStorage.removeItem(AVAILABLE_UPDATES_KEY);
      }
    } catch (error) {
      console.error("[ERROR] 保存更新状态失败:", error);
    }
  }, [availableUpdates]);

  useEffect(() => {
    try {
      if (availablePluginUpdates.size > 0) {
        const obj = Object.fromEntries(availablePluginUpdates);
        localStorage.setItem(AVAILABLE_PLUGIN_UPDATES_KEY, JSON.stringify(obj));
      } else {
        localStorage.removeItem(AVAILABLE_PLUGIN_UPDATES_KEY);
      }
    } catch (error) {
      console.error("[ERROR] 保存插件更新状态失败:", error);
    }
  }, [availablePluginUpdates]);

  useEffect(() => {
    try {
      if (availableMarketplaceUpdates.size > 0) {
        const obj = Object.fromEntries(availableMarketplaceUpdates);
        localStorage.setItem(AVAILABLE_MARKETPLACE_UPDATES_KEY, JSON.stringify(obj));
      } else {
        localStorage.removeItem(AVAILABLE_MARKETPLACE_UPDATES_KEY);
      }
    } catch (error) {
      console.error("[ERROR] 保存 Marketplace 更新状态失败:", error);
    }
  }, [availableMarketplaceUpdates]);

  useEffect(() => {
    if (!installedSkills || availableUpdates.size === 0) return;
    const installedSkillIds = new Set(installedSkills.map((skill) => skill.id));
    const needsCleanup = Array.from(availableUpdates.keys()).some(
      (skillId) => !installedSkillIds.has(skillId)
    );
    if (needsCleanup) {
      setAvailableUpdates((prev) => {
        const newMap = new Map(prev);
        for (const skillId of newMap.keys()) {
          if (!installedSkillIds.has(skillId)) {
            newMap.delete(skillId);
          }
        }
        return newMap;
      });
    }
  }, [installedSkills, availableUpdates]);

  useEffect(() => {
    if (!allPlugins || availablePluginUpdates.size === 0) return;
    const installedPluginIds = new Set(allPlugins.filter((p) => p.installed).map((p) => p.id));
    const needsCleanup = Array.from(availablePluginUpdates.keys()).some(
      (pluginId) => !installedPluginIds.has(pluginId)
    );
    if (needsCleanup) {
      setAvailablePluginUpdates((prev) => {
        const newMap = new Map(prev);
        for (const pluginId of newMap.keys()) {
          if (!installedPluginIds.has(pluginId)) {
            newMap.delete(pluginId);
          }
        }
        return newMap;
      });
    }
  }, [allPlugins, availablePluginUpdates]);

  useEffect(() => {
    if (!claudeMarketplaces || availableMarketplaceUpdates.size === 0) return;
    const marketplaceNames = new Set(claudeMarketplaces.map((m) => m.name));
    const needsCleanup = Array.from(availableMarketplaceUpdates.keys()).some(
      (name) => !marketplaceNames.has(name)
    );
    if (needsCleanup) {
      setAvailableMarketplaceUpdates((prev) => {
        const newMap = new Map(prev);
        for (const name of newMap.keys()) {
          if (!marketplaceNames.has(name)) {
            newMap.delete(name);
          }
        }
        return newMap;
      });
    }
  }, [claudeMarketplaces, availableMarketplaceUpdates]);

  const checkUpdatesWithRefresh = async (
    options?: { silent?: boolean }
  ): Promise<{ count: number; error?: string }> => {
    try {
      // 第一步：刷新本地技能
      setIsScanning(true);
      const localSkills = await api.scanLocalSkills();
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
      if (!options?.silent) {
        appToast.success(t("skills.installedPage.scanCompleted", { count: localSkills.length }));
      }
      setIsScanning(false);

      // 第二步：检查更新
      setIsCheckingUpdates(true);
      const updates = await api.checkSkillsUpdates();
      const updateMap = new Map(updates.map(([skillId, latestSha]) => [skillId, latestSha]));
      setAvailableUpdates(updateMap);
      if (!options?.silent) {
        if (updates.length > 0) {
          appToast.success(t("skills.installedPage.updatesFound", { count: updates.length }));
        } else {
          appToast.success(t("skills.installedPage.noUpdates"));
        }
      }
      return { count: updates.length };
    } catch (error: any) {
      const message = error?.message || String(error);
      if (!options?.silent) {
        if (isScanning) {
          appToast.error(t("skills.installedPage.scanFailed", { error: message }));
        } else {
          appToast.error(t("skills.installedPage.checkUpdatesFailed", { error: message }));
        }
      }
      return { count: 0, error: message };
    } finally {
      setIsScanning(false);
      setIsCheckingUpdates(false);
    }
  };

  const checkPluginUpdates = async (
    options?: { silent?: boolean }
  ): Promise<{ count: number; error?: string }> => {
    if (isCheckingPluginUpdates) return { count: 0 };
    setIsCheckingPluginUpdates(true);
    try {
      const updates = await api.checkPluginsUpdates();
      setAvailablePluginUpdates(new Map(updates));
      await queryClient.invalidateQueries({ queryKey: ["plugins"] });
      if (!options?.silent) {
        appToast.success(t("plugins.updates.checked", { count: updates.length }));
      }
      return { count: updates.length };
    } catch (error: any) {
      const message = error?.message || String(error);
      if (!options?.silent) {
        appToast.error(t("plugins.updates.checkFailed", { error: message }));
      }
      return { count: 0, error: message };
    } finally {
      setIsCheckingPluginUpdates(false);
    }
  };

  const checkMarketplaceUpdates = async (
    options?: { silent?: boolean }
  ): Promise<{ count: number; error?: string }> => {
    if (isCheckingMarketplaceUpdates) return { count: 0 };
    setIsCheckingMarketplaceUpdates(true);
    try {
      const updates = await api.checkMarketplacesUpdates();
      setAvailableMarketplaceUpdates(new Map(updates));
      await queryClient.invalidateQueries({ queryKey: ["claudeMarketplaces"] });
      if (!options?.silent) {
        appToast.success(t("plugins.marketplaces.updates.checked", { count: updates.length }));
      }
      return { count: updates.length };
    } catch (error: any) {
      const message = error?.message || String(error);
      if (!options?.silent) {
        appToast.error(
          t("plugins.marketplaces.updates.checkFailed", { error: message })
        );
      }
      return { count: 0, error: message };
    } finally {
      setIsCheckingMarketplaceUpdates(false);
    }
  };

  const checkAllUpdates = async () => {
    if (
      isCheckingAllUpdates ||
      isScanning ||
      isCheckingUpdates ||
      isCheckingPluginUpdates ||
      isCheckingMarketplaceUpdates
    ) {
      return;
    }
    setIsCheckingAllUpdates(true);
    try {
      const skillsResult = await checkUpdatesWithRefresh({ silent: true });
      const pluginsResult = await checkPluginUpdates({ silent: true });
      const marketplacesResult = await checkMarketplaceUpdates({ silent: true });
      const skillsCount = skillsResult?.count ?? 0;
      const pluginsCount = pluginsResult?.count ?? 0;
      const marketplacesCount = marketplacesResult?.count ?? 0;
      const total = skillsCount + pluginsCount + marketplacesCount;
      const failures: string[] = [];
      if (skillsResult?.error) failures.push(t("installed.checkUpdatesTargets.skills"));
      if (pluginsResult?.error) failures.push(t("installed.checkUpdatesTargets.plugins"));
      if (marketplacesResult?.error) failures.push(t("installed.checkUpdatesTargets.marketplaces"));
      if (failures.length > 0) {
        const separator = i18n.language === "zh" ? "、" : ", ";
        appToast.error(
          t("installed.checkUpdatesFailed", { targets: failures.join(separator) })
        );
        return;
      }
      if (total > 0) {
        setActiveTab("all");
        setSelectedRepository("all");
        setSearchQuery("");
        setShowUpdatesOnly(true);
        appToast.success(
          t("installed.checkUpdatesSummary", {
            skills: skillsCount,
            plugins: pluginsCount,
            marketplaces: marketplacesCount,
          })
        );
      } else {
        setShowUpdatesOnly(false);
        appToast.success(t("installed.checkUpdatesAllUpToDate"));
      }
    } finally {
      setIsCheckingAllUpdates(false);
    }
  };

  const updatePlugin = async (pluginId: string) => {
    if (updatingPluginId) return;
    setUpdatingPluginId(pluginId);
    try {
      const result = await api.updatePlugin(pluginId);
      const isUpdated = result.status === "updated";
      const isAlreadyLatest = result.status === "already_latest";

      if (isUpdated) {
        appToast.success(t("plugins.updates.updated"));
      } else if (isAlreadyLatest) {
        appToast.info(t("plugins.updates.alreadyLatest"));
      } else {
        appToast.error(t("plugins.updates.updateFailed"));
      }

      if (isUpdated || isAlreadyLatest) {
        setAvailablePluginUpdates((prev) => {
          const newMap = new Map(prev);
          newMap.delete(pluginId);
          return newMap;
        });
        await queryClient.invalidateQueries({ queryKey: ["plugins"] });
      }
    } catch (error: any) {
      appToast.error(
        t("plugins.updates.updateFailedWithError", { error: error.message || String(error) })
      );
    } finally {
      setUpdatingPluginId(null);
    }
  };

  const updateMarketplace = async (marketplaceName: string) => {
    if (updatingMarketplaceName) return;
    setUpdatingMarketplaceName(marketplaceName);
    try {
      const result = await api.updateMarketplace(marketplaceName);
      if (result.success) {
        appToast.success(t("plugins.marketplaces.updates.updated"));
        setAvailableMarketplaceUpdates((prev) => {
          const newMap = new Map(prev);
          newMap.delete(marketplaceName);
          return newMap;
        });
        await queryClient.invalidateQueries({ queryKey: ["claudeMarketplaces"] });
        await queryClient.invalidateQueries({ queryKey: ["plugins"] });
      } else {
        appToast.error(t("plugins.marketplaces.updates.updateFailed"));
      }
    } catch (error: any) {
      appToast.error(
        t("plugins.marketplaces.updates.updateFailedWithError", {
          error: error.message || String(error),
        })
      );
    } finally {
      setUpdatingMarketplaceName(null);
    }
  };

  const normalizedInstalledSkills = useMemo(
    () => normalizeInstalledSkills(installedSkills ?? []),
    [installedSkills]
  );

  const { data: skillPluginUpgradeCandidates = [] } = useQuery<SkillPluginUpgradeCandidate[]>({
    queryKey: ["skillPluginUpgradeCandidates"],
    queryFn: () => api.getSkillPluginUpgradeCandidates(),
    enabled: activeTab === "skills" || activeTab === "all",
    staleTime: 60_000,
  });

  const skillPluginUpgradeByName = useMemo(() => {
    const map = new Map<string, SkillPluginUpgradeCandidate>();
    for (const candidate of skillPluginUpgradeCandidates) {
      if (!candidate.skill_name) continue;
      map.set(candidate.skill_name.toLowerCase(), candidate);
    }
    return map;
  }, [skillPluginUpgradeCandidates]);

  const installedPlugins = useMemo(() => {
    return allPlugins.filter((plugin) => plugin.installed);
  }, [allPlugins]);

  const isLoading = isSkillsLoading || isPluginsLoading || isMarketplacesLoading;
  const updateCounts = useMemo(
    () => ({
      skills: availableUpdates.size,
      plugins: availablePluginUpdates.size,
      marketplaces: availableMarketplaceUpdates.size,
      total: availableUpdates.size + availablePluginUpdates.size + availableMarketplaceUpdates.size,
    }),
    [availableMarketplaceUpdates, availablePluginUpdates, availableUpdates]
  );

  const installedMarketplaces = useMemo<InstalledMarketplace[]>(() => {
    const byMarketplace = new Map<string, { name: string; repoUrl: string; plugins: Plugin[] }>();

    const upsertMarketplace = (name: string, repoUrl: string) => {
      const existing = byMarketplace.get(name);
      if (existing) {
        if (!existing.repoUrl && repoUrl) existing.repoUrl = repoUrl;
        return;
      }
      byMarketplace.set(name, { name, repoUrl, plugins: [] });
    };

    // 只展示“已安装/已配置”的 marketplaces：
    // 1) Claude CLI 已配置的 marketplaces（包含非本程序添加的）
    claudeMarketplaces.forEach((m) => {
      const repoUrl =
        m.repository_url ||
        (m.repo ? (m.repo.startsWith("http") ? m.repo : `https://github.com/${m.repo}`) : "");
      upsertMarketplace(m.name, repoUrl);
    });

    // 2) 回退：若 CLI 列表缺失，也至少展示已安装插件所属的 marketplace
    installedPlugins.forEach((plugin) => {
      if (!plugin.marketplace_name) return;
      upsertMarketplace(plugin.marketplace_name, plugin.repository_url || "");
    });

    // 再合并 DB 里的插件信息与安装数量
    allPlugins.forEach((plugin) => {
      if (!plugin.marketplace_name) return;
      const key = plugin.marketplace_name;
      const existing = byMarketplace.get(key);
      if (!existing) return;
      existing.plugins.push(plugin);
      if (!existing.repoUrl && plugin.repository_url) existing.repoUrl = plugin.repository_url;
    });

    return Array.from(byMarketplace.values())
      .map((marketplace) => {
        const installedCount = marketplace.plugins.filter((p) => p.installed).length;
        return {
          ...marketplace,
          installedCount,
          totalCount: marketplace.plugins.length,
        };
      })
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [allPlugins, claudeMarketplaces, installedPlugins]);
  const pageBusyMessage = useMemo(() => {
    if (
      isCheckingAllUpdates ||
      isScanning ||
      isCheckingUpdates ||
      isCheckingPluginUpdates ||
      isCheckingMarketplaceUpdates
    ) {
      return t("installed.busy.checkUpdates");
    }
    if (preparingUpdateSkillId) {
      const skill = normalizedInstalledSkills.find((item) => item.id === preparingUpdateSkillId);
      return t("installed.busy.prepareSkillUpdate", { name: skill?.name ?? "" });
    }
    if (confirmingUpdateSkillId) {
      const skill = normalizedInstalledSkills.find((item) => item.id === confirmingUpdateSkillId);
      return t("installed.busy.applySkillUpdate", { name: skill?.name ?? "" });
    }
    if (uninstallingSkillId) {
      const skill = normalizedInstalledSkills.find((item) => item.id === uninstallingSkillId);
      return t("installed.busy.uninstallSkill", { name: skill?.name ?? "" });
    }
    if (uninstallingPluginId) {
      const plugin = installedPlugins.find((item) => item.id === uninstallingPluginId);
      return t("installed.busy.uninstallPlugin", { name: plugin?.name ?? "" });
    }
    if (updatingPluginId) {
      const plugin = installedPlugins.find((item) => item.id === updatingPluginId);
      return t("installed.busy.updatePlugin", { name: plugin?.name ?? "" });
    }
    if (updatingMarketplaceName) {
      return t("installed.busy.updateMarketplace", { name: updatingMarketplaceName });
    }
    if (removingMarketplaceName) {
      return t("installed.busy.removeMarketplace", { name: removingMarketplaceName });
    }
    return null;
  }, [
    confirmingUpdateSkillId,
    installedPlugins,
    isCheckingAllUpdates,
    isCheckingMarketplaceUpdates,
    isCheckingPluginUpdates,
    isCheckingUpdates,
    isScanning,
    normalizedInstalledSkills,
    preparingUpdateSkillId,
    removingMarketplaceName,
    t,
    uninstallingPluginId,
    uninstallingSkillId,
    updatingMarketplaceName,
    updatingPluginId,
  ]);

  useEffect(() => {
    if (updateCounts.total === 0 && showUpdatesOnly) {
      setShowUpdatesOnly(false);
    }
  }, [showUpdatesOnly, updateCounts.total]);

  const tabCounts = useMemo(
    () => ({
      all: normalizedInstalledSkills.length + installedPlugins.length + installedMarketplaces.length,
      skills: normalizedInstalledSkills.length,
      plugins: installedPlugins.length,
      marketplaces: installedMarketplaces.length,
    }),
    [installedMarketplaces.length, installedPlugins.length, normalizedInstalledSkills.length]
  );

  const repositoryOptions: CyberSelectOption[] = useMemo(() => {
    if (activeTab === "marketplaces") return [];

    const items =
      activeTab === "skills"
        ? normalizedInstalledSkills.map((skill) => ({ owner: skill.repository_owner || "unknown" }))
        : activeTab === "plugins"
          ? installedPlugins.map((plugin) => ({ owner: plugin.repository_owner || "unknown" }))
          : [
              ...normalizedInstalledSkills.map((skill) => ({
                owner: skill.repository_owner || "unknown",
              })),
              ...installedPlugins.map((plugin) => ({
                owner: plugin.repository_owner || "unknown",
              })),
              ...installedMarketplaces.map((marketplace) => ({
                owner: getMarketplaceOwner(marketplace.repoUrl),
              })),
            ];

    if (!items || items.length === 0) {
      return [{ value: "all", label: `${t("skills.marketplace.allSources")} (0)` }];
    }

    const ownerMap = new Map<string, number>();
    items.forEach((item) => {
      ownerMap.set(item.owner, (ownerMap.get(item.owner) || 0) + 1);
    });

    const repos = Array.from(ownerMap.entries())
      .map(([owner, count]) => ({
        owner,
        count,
        displayName: owner === "local" ? t("skills.marketplace.localRepo") : `@${owner}`,
      }))
      .sort((a, b) => a.displayName.localeCompare(b.displayName));

    return [
      { value: "all", label: `${t("skills.marketplace.allSources")} (${items.length})` },
      ...repos.map((repo) => ({
        value: repo.owner,
        label: `${repo.displayName} (${repo.count})`,
      })),
    ];
  }, [activeTab, installedPlugins, installedMarketplaces, normalizedInstalledSkills, i18n.language, t]);

  const filteredSkills = useMemo(() => {
    let items = normalizedInstalledSkills;
    const query = searchQuery.trim().toLowerCase();

    if (selectedRepository !== "all") {
      items = items.filter((skill) => (skill.repository_owner || "unknown") === selectedRepository);
    }

    if (showUpdatesOnly) {
      items = items.filter((skill) => availableUpdates.has(skill.id));
    }

    if (query) {
      items = items.filter((skill) => {
        const nameMatch = skill.name.toLowerCase().includes(query);
        const descriptionMatch = skill.description?.toLowerCase().includes(query);
        return nameMatch || descriptionMatch;
      });
    }

    return [...items].sort((a, b) => {
      const updateDelta = Number(availableUpdates.has(b.id)) - Number(availableUpdates.has(a.id));
      if (updateDelta !== 0) return updateDelta;
      if (query) {
        const aRank = a.name.toLowerCase().includes(query) ? 0 : 1;
        const bRank = b.name.toLowerCase().includes(query) ? 0 : 1;
        if (aRank !== bRank) return aRank - bRank;
      }
      const timeA = a.installed_at ? new Date(a.installed_at).getTime() : 0;
      const timeB = b.installed_at ? new Date(b.installed_at).getTime() : 0;
      if (timeA !== timeB) return timeB - timeA;
      return a.name.localeCompare(b.name);
    });
  }, [availableUpdates, normalizedInstalledSkills, searchQuery, selectedRepository, showUpdatesOnly]);

  const filteredPlugins = useMemo(() => {
    let items = installedPlugins;
    const query = searchQuery.trim().toLowerCase();

    if (selectedRepository !== "all") {
      items = items.filter(
        (plugin) => (plugin.repository_owner || "unknown") === selectedRepository
      );
    }

    if (showUpdatesOnly) {
      items = items.filter((plugin) => availablePluginUpdates.has(plugin.id));
    }

    if (query) {
      items = items.filter((plugin) => {
        const nameMatch = plugin.name.toLowerCase().includes(query);
        const descriptionMatch = plugin.description?.toLowerCase().includes(query);
        return nameMatch || descriptionMatch;
      });
    }

    return [...items].sort((a, b) => {
      const updateDelta =
        Number(availablePluginUpdates.has(b.id)) - Number(availablePluginUpdates.has(a.id));
      if (updateDelta !== 0) return updateDelta;
      if (query) {
        const aRank = a.name.toLowerCase().includes(query) ? 0 : 1;
        const bRank = b.name.toLowerCase().includes(query) ? 0 : 1;
        if (aRank !== bRank) return aRank - bRank;
      }
      const timeA = a.installed_at ? new Date(a.installed_at).getTime() : 0;
      const timeB = b.installed_at ? new Date(b.installed_at).getTime() : 0;
      if (timeA !== timeB) return timeB - timeA;
      return a.name.localeCompare(b.name);
    });
  }, [availablePluginUpdates, installedPlugins, searchQuery, selectedRepository, showUpdatesOnly]);

  const filteredMarketplaces = useMemo(() => {
    let items = installedMarketplaces;
    const query = searchQuery.trim().toLowerCase();

    if (showUpdatesOnly) {
      items = items.filter((marketplace) => availableMarketplaceUpdates.has(marketplace.name));
    }

    if (query) {
      items = items.filter((m) => {
        const description = marketplaceDescriptions.get(m.name);
        return (
          m.name.toLowerCase().includes(query) ||
          (description ? description.toLowerCase().includes(query) : false)
        );
      });
    }

    return [...items].sort((a, b) => {
      const updateDelta =
        Number(availableMarketplaceUpdates.has(b.name)) -
        Number(availableMarketplaceUpdates.has(a.name));
      if (updateDelta !== 0) return updateDelta;
      if (query) {
        const aRank = a.name.toLowerCase().includes(query) ? 0 : 1;
        const bRank = b.name.toLowerCase().includes(query) ? 0 : 1;
        if (aRank !== bRank) return aRank - bRank;
      }
      return a.name.localeCompare(b.name);
    });
  }, [
    availableMarketplaceUpdates,
    installedMarketplaces,
    marketplaceDescriptions,
    searchQuery,
    showUpdatesOnly,
  ]);

  const filteredAllItems = useMemo<InstalledEntry[]>(() => {
    const items: InstalledEntry[] = [
      ...normalizedInstalledSkills.map((skill): InstalledEntry => ({ kind: "skill", item: skill })),
      ...installedPlugins.map((plugin): InstalledEntry => ({ kind: "plugin", item: plugin })),
      ...installedMarketplaces.map((marketplace): InstalledEntry => ({
        kind: "marketplace",
        item: marketplace,
      })),
    ];

    if (!items.length) return [];

    const query = searchQuery.trim().toLowerCase();

    let filtered = items.filter((entry) => {
      const owner =
        entry.kind === "marketplace"
          ? getMarketplaceOwner(entry.item.repoUrl)
          : entry.item.repository_owner || "unknown";
      const matchesRepo = selectedRepository === "all" || owner === selectedRepository;

      const description =
        entry.kind === "marketplace"
          ? marketplaceDescriptions.get(entry.item.name)
          : entry.item.description;
      const marketplaceName =
        entry.kind === "plugin" ? entry.item.marketplace_name?.toLowerCase() ?? "" : "";

      const matchesSearch =
        !query ||
        entry.item.name.toLowerCase().includes(query) ||
        (description ? description.toLowerCase().includes(query) : false) ||
        (marketplaceName && marketplaceName.includes(query));

      const matchesUpdate =
        !showUpdatesOnly ||
        (entry.kind === "skill" && availableUpdates.has(entry.item.id)) ||
        (entry.kind === "plugin" && availablePluginUpdates.has(entry.item.id)) ||
        (entry.kind === "marketplace" && availableMarketplaceUpdates.has(entry.item.name));

      return matchesSearch && matchesRepo && matchesUpdate;
    });

    return [...filtered].sort((a, b) => {
      const aHasUpdate =
        (a.kind === "skill" && availableUpdates.has(a.item.id)) ||
        (a.kind === "plugin" && availablePluginUpdates.has(a.item.id)) ||
        (a.kind === "marketplace" && availableMarketplaceUpdates.has(a.item.name));
      const bHasUpdate =
        (b.kind === "skill" && availableUpdates.has(b.item.id)) ||
        (b.kind === "plugin" && availablePluginUpdates.has(b.item.id)) ||
        (b.kind === "marketplace" && availableMarketplaceUpdates.has(b.item.name));
      const updateDelta = Number(bHasUpdate) - Number(aHasUpdate);
      if (updateDelta !== 0) return updateDelta;
      if (query) {
        const aRank = a.item.name.toLowerCase().includes(query) ? 0 : 1;
        const bRank = b.item.name.toLowerCase().includes(query) ? 0 : 1;
        if (aRank !== bRank) return aRank - bRank;
      }
      return a.item.name.localeCompare(b.item.name);
    });
  }, [
    availableMarketplaceUpdates,
    availablePluginUpdates,
    availableUpdates,
    installedMarketplaces,
    installedPlugins,
    marketplaceDescriptions,
    normalizedInstalledSkills,
    searchQuery,
    selectedRepository,
    showUpdatesOnly,
  ]);

  const focusUpdateItems = (tab: "all" | "skills" | "plugins" | "marketplaces") => {
    setActiveTab(tab);
    setShowUpdatesOnly(true);
    setSelectedRepository("all");
    setSearchQuery("");
  };

  const renderSkillCard = (skill: Skill, index: number) => {
    const upgradeCandidate = skillPluginUpgradeByName.get(skill.name.toLowerCase());
    return (
      <SkillCard
        key={`skill-${skill.id}`}
        skill={skill}
        index={index}
        pluginUpgradeCandidate={upgradeCandidate}
        onShowPluginUpgrade={() => {
          if (upgradeCandidate) setPendingSkillPluginUpgrade(upgradeCandidate);
        }}
        onUninstall={async () => {
          setInstalledOps((prev) => ({ ...prev, uninstallingSkillId: skill.id }));
          const errors: string[] = [];
          try {
            try {
              await uninstallMutation.mutateAsync(skill.id);
            } catch (error: any) {
              errors.push(error?.message || String(error));
            }
          } finally {
            setInstalledOps((prev) => ({ ...prev, uninstallingSkillId: null }));
          }
          if (errors.length === 0) {
            appToast.success(t("skills.toast.uninstalled"));
          } else {
            appToast.error(`${t("skills.toast.uninstallFailed")}: ${errors[0]}`);
          }
        }}
        onUninstallPath={(path: string) => {
          uninstallPathMutation.mutate(
            { skillId: skill.id, path },
            {
              onSuccess: () => appToast.success(t("skills.toast.uninstalled")),
              onError: (error: any) =>
                appToast.error(`${t("skills.toast.uninstallFailed")}: ${error.message || error}`),
            }
          );
        }}
        onUpdate={async () => {
          try {
            setPreparingUpdateSkillId(skill.id);
            const [report, conflicts] = await api.prepareSkillUpdate(skill.id, i18n.language);
            setPreparingUpdateSkillId(null);
            setPendingUpdate({ skill, report, conflicts });
          } catch (error: any) {
            setPreparingUpdateSkillId(null);
            appToast.error(`${t("skills.toast.updateFailed")}: ${error.message || error}`);
          }
        }}
        hasUpdate={availableUpdates.has(skill.id)}
        isUninstalling={uninstallingSkillId === skill.id}
        isPreparingUpdate={preparingUpdateSkillId === skill.id}
        isApplyingUpdate={confirmingUpdateSkillId === skill.id}
        isAnyOperationPending={
          uninstallMutation.isPending ||
          uninstallPathMutation.isPending ||
          preparingUpdateSkillId !== null ||
          confirmingUpdateSkillId !== null ||
          uninstallingPluginId !== null ||
          updatingPluginId !== null ||
          updatingMarketplaceName !== null
        }
        t={t}
      />
    );
  };

  const renderPluginCard = (plugin: Plugin) => (
    <InstalledPluginCard
      key={`plugin-${plugin.id}`}
      plugin={plugin}
      hasUpdate={availablePluginUpdates.has(plugin.id)}
      latestVersion={availablePluginUpdates.get(plugin.id)}
      isUpdating={updatingPluginId === plugin.id}
      isUninstalling={uninstallingPluginId === plugin.id}
      isAnyOperationPending={
        uninstallMutation.isPending ||
        uninstallPathMutation.isPending ||
        preparingUpdateSkillId !== null ||
        confirmingUpdateSkillId !== null ||
        uninstallingPluginId !== null ||
        removingMarketplaceName !== null ||
        updatingPluginId !== null ||
        updatingMarketplaceName !== null
      }
      onUpdate={() => updatePlugin(plugin.id)}
      onUninstall={async () => {
        try {
          setInstalledOps((prev) => ({ ...prev, uninstallingPluginId: plugin.id }));
          const result = await uninstallPluginMutation.mutateAsync(plugin.id);
          if (result.success) {
            appToast.success(t("plugins.toast.uninstalled"));
          } else {
            appToast.error(t("plugins.toast.uninstallFailed"));
          }
        } catch (error: any) {
          appToast.error(`${t("plugins.toast.uninstallFailed")}: ${error.message || error}`);
        } finally {
          setInstalledOps((prev) => ({ ...prev, uninstallingPluginId: null }));
        }
      }}
    />
  );

  const renderMarketplaceCard = (marketplace: InstalledMarketplace) => (
    <InstalledMarketplaceCard
      key={`marketplace-${marketplace.name}`}
      marketplaceName={marketplace.name}
      marketplaceRepo={marketplace.repoUrl}
      description={marketplaceDescriptions.get(marketplace.name)}
      installedCount={marketplace.installedCount}
      totalCount={marketplace.totalCount}
      hasUpdate={availableMarketplaceUpdates.has(marketplace.name)}
      latestHead={availableMarketplaceUpdates.get(marketplace.name)}
      isUpdating={updatingMarketplaceName === marketplace.name}
      isRemoving={removingMarketplaceName === marketplace.name}
      isAnyOperationPending={
        uninstallingPluginId !== null ||
        uninstallMutation.isPending ||
        uninstallPathMutation.isPending ||
        removingMarketplaceName !== null ||
        updatingPluginId !== null ||
        updatingMarketplaceName !== null
      }
      onUpdate={() => updateMarketplace(marketplace.name)}
      onRemove={() => {
        const installedPluginNames = marketplace.plugins
          .filter((p) => p.installed)
          .map((p) => p.name)
          .sort((a, b) => a.localeCompare(b));
        setInstalledOps((prev) => ({
          ...prev,
          pendingMarketplaceRemove: {
            marketplaceName: marketplace.name,
            marketplaceRepo: marketplace.repoUrl,
            installedPluginNames,
          },
        }));
      }}
    />
  );

  const pluginUpgradeCommands = useMemo(() => {
    if (!pendingSkillPluginUpgrade) return "";
    const lines: string[] = [];
    if (pendingSkillPluginUpgrade.marketplace_add_command) {
      lines.push(pendingSkillPluginUpgrade.marketplace_add_command);
    } else {
      const repoArg =
        pendingSkillPluginUpgrade.marketplace_repo ||
        pendingSkillPluginUpgrade.marketplace_repository_url ||
        "";
      if (repoArg) lines.push(`claude plugin marketplace add ${repoArg}`);
    }
    lines.push(`claude plugin install ${pendingSkillPluginUpgrade.plugin_id}`);
    return lines.join("\n");
  }, [pendingSkillPluginUpgrade]);

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
                <h1 className="text-headline text-foreground">{t("nav.installed")}</h1>
                <button
                  onClick={checkAllUpdates}
                  disabled={
                    isCheckingAllUpdates ||
                    isScanning ||
                    isCheckingUpdates ||
                    isCheckingPluginUpdates ||
                    isCheckingMarketplaceUpdates
                  }
                  className="apple-button-primary h-10 px-5 flex items-center gap-2 disabled:opacity-50"
                >
                  {isCheckingAllUpdates ||
                  isScanning ||
                  isCheckingUpdates ||
                  isCheckingPluginUpdates ||
                  isCheckingMarketplaceUpdates ? (
                    <>
                      <Loader2 className="w-4 h-4 animate-spin" />
                      {t("skills.installedPage.checkingUpdates")}
                    </>
                  ) : (
                    <>
                      <RefreshCw className="w-4 h-4" />
                      {t("skills.installedPage.checkUpdates")}
                    </>
                  )}
                </button>
              </div>
            </div>

            <div className="flex items-center gap-2 mb-4">
              <InstalledTabButton
                active={activeTab === "all"}
                onClick={() => {
                  setActiveTab("all");
                  setSelectedRepository("all");
                  setSearchQuery("");
                }}
                label={t("installed.tabs.all", { count: tabCounts.all })}
              />
              <InstalledTabButton
                active={activeTab === "skills"}
                onClick={() => {
                  setActiveTab("skills");
                  setSelectedRepository("all");
                  setSearchQuery("");
                }}
                label={t("installed.tabs.skills", { count: tabCounts.skills })}
              />
              <InstalledTabButton
                active={activeTab === "plugins"}
                onClick={() => {
                  setActiveTab("plugins");
                  setSelectedRepository("all");
                  setSearchQuery("");
                }}
                label={t("installed.tabs.plugins", { count: tabCounts.plugins })}
              />
              <InstalledTabButton
                active={activeTab === "marketplaces"}
                onClick={() => {
                  setActiveTab("marketplaces");
                  setSelectedRepository("all");
                  setSearchQuery("");
                }}
                label={t("installed.tabs.marketplaces", { count: tabCounts.marketplaces })}
              />
            </div>

            <div className="flex gap-3 items-center flex-wrap">
              <div className="relative flex-1 min-w-[300px]">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <input
                  type="text"
                  placeholder={
                    activeTab === "skills"
                      ? t("skills.installedPage.search")
                      : activeTab === "plugins"
                        ? t("plugins.search")
                        : activeTab === "marketplaces"
                          ? t("installed.marketplaces.search")
                          : t("installed.search")
                  }
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="apple-input w-full h-10 pl-11 pr-4"
                />
              </div>

              {activeTab !== "marketplaces" && (
                <CyberSelect
                  value={selectedRepository}
                  onChange={setSelectedRepository}
                  options={repositoryOptions}
                  className="min-w-[200px]"
                />
              )}
            </div>

            {updateCounts.total > 0 && (
              <div className="mt-4 flex flex-wrap items-center gap-2 rounded-2xl border border-primary/15 bg-primary/5 px-4 py-3">
                <span className="text-sm font-medium text-foreground">
                  {t("installed.updatesFocus.title", { count: updateCounts.total })}
                </span>
                <button
                  type="button"
                  onClick={() => focusUpdateItems("all")}
                  className={`h-8 rounded-full px-3 text-xs transition-colors ${
                    activeTab === "all" && showUpdatesOnly
                      ? "bg-primary text-primary-foreground"
                      : "bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("installed.updatesFocus.all", { count: updateCounts.total })}
                </button>
                <button
                  type="button"
                  onClick={() => focusUpdateItems("skills")}
                  className={`h-8 rounded-full px-3 text-xs transition-colors ${
                    activeTab === "skills" && showUpdatesOnly
                      ? "bg-primary text-primary-foreground"
                      : "bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("installed.updatesFocus.skills", { count: updateCounts.skills })}
                </button>
                <button
                  type="button"
                  onClick={() => focusUpdateItems("plugins")}
                  className={`h-8 rounded-full px-3 text-xs transition-colors ${
                    activeTab === "plugins" && showUpdatesOnly
                      ? "bg-primary text-primary-foreground"
                      : "bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("installed.updatesFocus.plugins", { count: updateCounts.plugins })}
                </button>
                <button
                  type="button"
                  onClick={() => focusUpdateItems("marketplaces")}
                  className={`h-8 rounded-full px-3 text-xs transition-colors ${
                    activeTab === "marketplaces" && showUpdatesOnly
                      ? "bg-primary text-primary-foreground"
                      : "bg-card text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t("installed.updatesFocus.marketplaces", {
                    count: updateCounts.marketplaces,
                  })}
                </button>
                <button
                  type="button"
                  onClick={() => setShowUpdatesOnly((value) => !value)}
                  className="ml-auto h-8 rounded-full border border-border bg-card px-3 text-xs text-muted-foreground transition-colors hover:text-foreground"
                >
                  {showUpdatesOnly
                    ? t("installed.updatesFocus.showAll")
                    : t("installed.updatesFocus.showOnly")}
                </button>
              </div>
            )}

            {pageBusyMessage && (
              <div className="mt-4">
                <PageBusyNotice message={pageBusyMessage} />
              </div>
            )}
          </div>
        </div>
      </div>

      <div
        ref={listContainerRef}
        aria-busy={pageBusyMessage ? "true" : "false"}
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
              <p className="text-sm text-muted-foreground">{t("skills.loading")}</p>
            </div>
          ) : activeTab === "all" && filteredAllItems.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5 auto-rows-fr">
              {filteredAllItems.map((entry, index) => {
                if (entry.kind === "skill") return renderSkillCard(entry.item, index);
                if (entry.kind === "plugin") return renderPluginCard(entry.item);
                return renderMarketplaceCard(entry.item);
              })}
            </div>
          ) : activeTab === "skills" && filteredSkills.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5 auto-rows-fr">
              {filteredSkills.map((skill, index) => renderSkillCard(skill, index))}
            </div>
          ) : activeTab === "plugins" && filteredPlugins.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5 auto-rows-fr">
              {filteredPlugins.map((plugin) => renderPluginCard(plugin))}
            </div>
          ) : activeTab === "marketplaces" && filteredMarketplaces.length > 0 ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5 auto-rows-fr">
              {filteredMarketplaces.map((marketplace) => renderMarketplaceCard(marketplace))}
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-20 apple-card">
              <div className="w-20 h-20 rounded-full bg-secondary flex items-center justify-center mb-5">
                {searchQuery || (activeTab !== "marketplaces" && selectedRepository !== "all") ? (
                  <SearchX className="w-10 h-10 text-muted-foreground" />
                ) : (
                  <Package className="w-10 h-10 text-muted-foreground" />
                )}
              </div>
              <p className="text-sm text-muted-foreground">
                {searchQuery || (activeTab !== "marketplaces" && selectedRepository !== "all")
                  ? t("installed.empty.noResults", { query: searchQuery })
                  : showUpdatesOnly
                    ? t("installed.empty.noUpdates")
                  : activeTab === "all"
                    ? t("installed.empty.all")
                    : activeTab === "skills"
                      ? t("skills.installedPage.empty")
                      : activeTab === "plugins"
                        ? t("installed.plugins.empty")
                        : t("installed.marketplaces.empty")}
              </p>
              {(searchQuery || (activeTab !== "marketplaces" && selectedRepository !== "all")) && (
                <button
                  onClick={() => {
                    setSearchQuery("");
                    setSelectedRepository("all");
                  }}
                  className="mt-5 apple-button-secondary"
                >
                  {t("installed.empty.clearFilters")}
                </button>
              )}
              {!searchQuery &&
                selectedRepository === "all" &&
                showUpdatesOnly && (
                  <button
                    onClick={() => setShowUpdatesOnly(false)}
                    className="mt-5 apple-button-secondary"
                  >
                    {t("installed.updatesFocus.showAll")}
                  </button>
                )}
            </div>
          )}
        </div>
      </div>

      <UpdateConfirmDialog
        open={pendingUpdate !== null}
        onClose={async () => {
          if (confirmingUpdateSkillId) return;
          if (pendingUpdate) {
            try {
              await api.cancelSkillUpdate(pendingUpdate.skill.id);
            } catch (error: any) {
              console.error("[ERROR] 取消更新失败:", error);
            }
          }
          setPendingUpdate(null);
        }}
        onConfirm={async (forceOverwrite: boolean) => {
          if (pendingUpdate) {
            try {
              setConfirmingUpdateSkillId(pendingUpdate.skill.id);
              await api.confirmSkillUpdate(
                pendingUpdate.skill.id,
                forceOverwrite,
                Boolean(pendingUpdate.report.partial_scan || pendingUpdate.report.skipped_files?.length)
              );
              await queryClient.refetchQueries({ queryKey: ["skills"] });
              await queryClient.refetchQueries({ queryKey: ["skills", "installed"] });
              await queryClient.refetchQueries({ queryKey: ["scanResults"] });
              setAvailableUpdates((prev) => {
                const newMap = new Map(prev);
                newMap.delete(pendingUpdate.skill.id);
                return newMap;
              });
              appToast.success(t("skills.toast.updateSuccess"));
            } catch (error: any) {
              appToast.error(`${t("skills.toast.updateFailed")}: ${error.message || error}`);
            } finally {
              setConfirmingUpdateSkillId(null);
            }
          }
          setPendingUpdate(null);
        }}
        isConfirming={pendingUpdate ? confirmingUpdateSkillId === pendingUpdate.skill.id : false}
        report={pendingUpdate?.report || null}
        conflicts={pendingUpdate?.conflicts || []}
        skillName={pendingUpdate?.skill.name || ""}
      />

      <AlertDialog
        open={pendingSkillPluginUpgrade !== null}
        onOpenChange={(open) => {
          if (!open) setPendingSkillPluginUpgrade(null);
        }}
      >
        <AlertDialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("skills.installedPage.upgradeDialog.title")}</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-4 pb-4">
                <div className="text-sm text-muted-foreground">
                  {t("skills.installedPage.upgradeDialog.description")}
                </div>

                {pendingSkillPluginUpgrade && (
                  <div className="space-y-2 text-sm">
                    <div>
                      <span className="text-blue-500 font-medium">
                        {t("skills.installedPage.upgradeDialog.plugin")}
                      </span>{" "}
                      {pendingSkillPluginUpgrade.plugin_id}
                      {pendingSkillPluginUpgrade.latest_version
                        ? ` (${pendingSkillPluginUpgrade.latest_version})`
                        : ""}
                    </div>

                    {pendingSkillPluginUpgrade.marketplace_repo && (
                      <div>
                        <span className="text-blue-500 font-medium">
                          {t("skills.installedPage.upgradeDialog.marketplaceRepo")}
                        </span>{" "}
                        {pendingSkillPluginUpgrade.marketplace_repo}
                      </div>
                    )}

                    <div className="mt-3">
                      <div className="text-blue-500 font-medium mb-2">
                        {t("skills.installedPage.upgradeDialog.commands")}
                      </div>
                      <div className="font-mono text-xs whitespace-pre-wrap p-3 bg-secondary/50 rounded-xl border border-border/60">
                        {pluginUpgradeCommands}
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>

          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setPendingSkillPluginUpgrade(null)}>
              {t("skills.marketplace.install.cancel")}
            </AlertDialogCancel>
            <button
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(pluginUpgradeCommands);
                  appToast.success(t("skills.installedPage.upgradeDialog.copied"));
                } catch (error: any) {
                  appToast.error(
                    t("skills.installedPage.upgradeDialog.copyFailed", {
                      error: error?.message || String(error),
                    })
                  );
                }
              }}
              disabled={!pluginUpgradeCommands}
              className="macos-button-primary disabled:opacity-50"
            >
              {t("skills.installedPage.upgradeDialog.copy")}
            </button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={pendingMarketplaceRemove !== null}
        onOpenChange={(open) => {
          if (!open) {
            setInstalledOps((prev) => ({ ...prev, pendingMarketplaceRemove: null }));
          }
        }}
      >
        <AlertDialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("plugins.confirmRemoveTitle")}</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-3">
                <div>
                  {t("plugins.confirmRemoveMessage", {
                    name: pendingMarketplaceRemove?.marketplaceName || "",
                    count: pendingMarketplaceRemove?.installedPluginNames.length || 0,
                  })}
                </div>

                {(pendingMarketplaceRemove?.installedPluginNames.length || 0) > 0 && (
                  <div className="p-3 rounded-lg bg-muted/40 border border-border/60">
                    <div className="text-sm font-medium mb-2">
                      {t("installed.marketplaces.dependentPlugins")}
                    </div>
                    <ul className="text-sm text-muted-foreground space-y-1 max-h-52 overflow-y-auto">
                      {pendingMarketplaceRemove?.installedPluginNames.slice(0, 30).map((name) => (
                        <li key={name}>- {name}</li>
                      ))}
                      {(pendingMarketplaceRemove?.installedPluginNames.length || 0) > 30 && (
                        <li className="text-xs">
                          {t("installed.marketplaces.andMore", {
                            count:
                              (pendingMarketplaceRemove?.installedPluginNames.length || 0) - 30,
                          })}
                        </li>
                      )}
                    </ul>
                  </div>
                )}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={removingMarketplaceName !== null}>
              {t("skills.cancel")}
            </AlertDialogCancel>
            <button
              onClick={async () => {
                if (!pendingMarketplaceRemove) return;
                const { marketplaceName, marketplaceRepo, installedPluginNames } =
                  pendingMarketplaceRemove;
                const installedCount = installedPluginNames.length;
                setInstalledOps((prev) => ({
                  ...prev,
                  removingMarketplaceName: marketplaceName,
                  pendingMarketplaceRemove: null,
                }));
                try {
                  const result = await removeMarketplaceMutation.mutateAsync({
                    marketplaceName,
                    marketplaceRepo,
                  });
                  if (result.success) {
                    appToast.success(
                      t("plugins.toast.marketplaceRemoved", { count: installedCount })
                    );
                  } else {
                    appToast.error(t("plugins.toast.marketplaceRemoveFailed"));
                  }
                } catch (error: any) {
                  appToast.error(
                    `${t("plugins.toast.marketplaceRemoveFailed")}: ${error.message || error}`
                  );
                } finally {
                  setInstalledOps((prev) => ({ ...prev, removingMarketplaceName: null }));
                }
              }}
              disabled={removingMarketplaceName !== null}
              className="apple-button-primary bg-red-500 hover:bg-red-600 disabled:opacity-50"
            >
              {removingMarketplaceName
                ? t("plugins.removingMarketplace")
                : t("plugins.removeMarketplace")}
            </button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function InstalledTabButton({
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

function InstalledMarketplaceCard({
  marketplaceName,
  marketplaceRepo,
  description,
  installedCount,
  totalCount,
  hasUpdate,
  latestHead,
  isUpdating,
  isRemoving,
  isAnyOperationPending,
  onUpdate,
  onRemove,
}: {
  marketplaceName: string;
  marketplaceRepo: string;
  description?: string;
  installedCount: number;
  totalCount: number;
  hasUpdate: boolean;
  latestHead?: string;
  isUpdating: boolean;
  isRemoving: boolean;
  isAnyOperationPending: boolean;
  onUpdate: () => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="apple-card p-6 group flex flex-col h-full">
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2.5 mb-1 flex-wrap">
            <h3 className="font-semibold text-foreground">{marketplaceName}</h3>
            <span className="text-xs px-2.5 py-1 rounded-full font-medium text-purple-600 bg-purple-500/10">
              Marketplace
            </span>
          </div>
        </div>

        <div className="flex gap-2 ml-4">
          {hasUpdate && (
            <button
              onClick={onUpdate}
              disabled={isAnyOperationPending}
              title={
                latestHead
                  ? t("plugins.marketplaces.updates.available", { version: latestHead })
                  : undefined
              }
              className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5"
            >
              {isUpdating ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  {t("plugins.marketplaces.updates.updating")}
                </>
              ) : (
                <>
                  <Download className="w-3.5 h-3.5" />
                  {t("plugins.marketplaces.updates.update")}
                </>
              )}
            </button>
          )}
          <button
            onClick={onRemove}
            disabled={isAnyOperationPending}
            className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
          >
            {isRemoving ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("plugins.removingMarketplace")}
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5" />
                {t("plugins.removeMarketplace")}
              </>
            )}
          </button>
        </div>
      </div>

      {description && (
        <p className="text-sm text-muted-foreground mb-4 leading-5 overflow-hidden [display:-webkit-box] [-webkit-line-clamp:3] [-webkit-box-orient:vertical]">
          {description}
        </p>
      )}

      <div className="text-sm text-muted-foreground mt-auto">
        <span className="text-blue-500 font-medium">
          {t("installed.marketplaces.pluginsCount")}
        </span>{" "}
        {installedCount} / {totalCount}
      </div>

      <div className="text-sm text-muted-foreground mt-2">
        <span className="text-blue-500 font-medium">{t("installed.marketplaces.source")}</span>{" "}
        {marketplaceRepo ? (
          <a
            href={marketplaceRepo}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-500 hover:text-blue-600 transition-colors break-all"
          >
            {marketplaceRepo}
          </a>
        ) : (
          <span>{t("skills.empty")}</span>
        )}
      </div>
    </div>
  );
}

interface SkillCardProps {
  skill: Skill;
  index: number;
  pluginUpgradeCandidate?: SkillPluginUpgradeCandidate;
  onShowPluginUpgrade: () => void;
  onUninstall: () => void;
  onUninstallPath: (path: string) => void;
  onUpdate: () => void;
  hasUpdate: boolean;
  isUninstalling: boolean;
  isPreparingUpdate: boolean;
  isApplyingUpdate: boolean;
  isAnyOperationPending: boolean;
  t: (key: string, options?: any) => string;
}

function CardCornerBadge({ kind, label }: { kind: "skill" | "plugin"; label: string }) {
  const className = kind === "skill" ? "bg-blue-600" : "bg-purple-600";

  return (
    <div
      style={{
        clipPath: "polygon(0 0, 100% 0, 100% 100%, 50% 72%, 0 100%)",
      }}
      className={`pointer-events-none absolute left-5 top-0 z-10 select-none px-2 pt-1 pb-3 text-xs font-semibold leading-none text-white shadow-md ${className}`}
    >
      {label}
    </div>
  );
}

function SkillCard({
  skill,
  pluginUpgradeCandidate,
  onShowPluginUpgrade,
  onUninstall,
  onUninstallPath,
  onUpdate,
  hasUpdate,
  isUninstalling,
  isPreparingUpdate,
  isApplyingUpdate,
  isAnyOperationPending,
  t,
}: SkillCardProps) {
  const descriptionRef = useRef<HTMLParagraphElement | null>(null);
  const [isDescriptionTruncated, setIsDescriptionTruncated] = useState(false);

  useLayoutEffect(() => {
    const element = descriptionRef.current;
    if (!element) return;

    const update = () => {
      setIsDescriptionTruncated(
        element.scrollHeight > element.clientHeight || element.scrollWidth > element.clientWidth
      );
    };

    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [skill.description]);

  return (
    <div className="apple-card p-6 pt-10 group flex flex-col h-full relative">
      <CardCornerBadge kind="skill" label={t("skills.badge")} />
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1">
          <div className="flex items-center gap-2.5 mb-1 flex-wrap">
            <h3 className="font-semibold text-foreground">{skill.name}</h3>
            <span
              className={`text-xs px-2.5 py-1 rounded-full font-medium ${
                skill.repository_owner === "local"
                  ? "text-muted-foreground bg-secondary"
                  : "text-blue-600 bg-blue-500/10"
              }`}
            >
              {formatRepositoryTag(skill)}
            </span>
            {pluginUpgradeCandidate && (
              <span className="text-xs px-2.5 py-1 rounded-full font-medium text-warning bg-warning/10">
                {t("skills.installedPage.upgradeBadge")}
              </span>
            )}
          </div>
        </div>

        <div className="flex gap-2 ml-4">
          {pluginUpgradeCandidate && (
            <button
              onClick={onShowPluginUpgrade}
              disabled={isAnyOperationPending}
              aria-label={`${t("skills.installedPage.upgradeToPlugin")}: ${skill.name}`}
              title={`${t("skills.installedPage.upgradeToPlugin")}: ${skill.name}`}
              className="apple-button-secondary h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
            >
              <Plug className="w-3.5 h-3.5" />
              {t("skills.installedPage.upgradeToPlugin")}
            </button>
          )}
          {hasUpdate && !skill.repository_owner?.includes("local") && (
            <button
              onClick={onUpdate}
              disabled={isAnyOperationPending}
              aria-label={`${t("skills.update")}: ${skill.name}`}
              title={`${t("skills.update")}: ${skill.name}`}
              className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5"
            >
              {isPreparingUpdate || isApplyingUpdate ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  {isApplyingUpdate
                    ? t("skills.installedPage.applyingUpdate")
                    : t("skills.installedPage.securityChecking")}
                </>
              ) : (
                <>
                  <Download className="w-3.5 h-3.5" />
                  {t("skills.update")}
                </>
              )}
            </button>
          )}
          <button
            onClick={onUninstall}
            disabled={isAnyOperationPending}
            aria-label={`${t("skills.uninstallAll")}: ${skill.name}`}
            title={`${t("skills.uninstallAll")}: ${skill.name}`}
            className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5"
          >
            {isUninstalling ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("skills.uninstalling")}
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5" />
                {t("skills.uninstallAll")}
              </>
            )}
          </button>
        </div>
      </div>

      {/* Description - 自动填充剩余空间 */}
      <div className="relative mb-4">
        <p
          ref={descriptionRef}
          title={isDescriptionTruncated && skill.description ? skill.description : undefined}
          className="text-sm text-muted-foreground leading-5 h-[3.75rem] overflow-hidden [display:-webkit-box] [-webkit-line-clamp:3] [-webkit-box-orient:vertical] peer"
        >
          {skill.description || t("skills.noDescription")}
        </p>
        {isDescriptionTruncated && skill.description && (
          <div className="apple-tooltip peer-hover:opacity-100 peer-hover:translate-y-0">
            {skill.description}
          </div>
        )}
      </div>

      {/* Repository - 固定在底部 */}
      <div className="text-sm text-muted-foreground mb-4">
        <span className="text-blue-500 font-medium">{t("skills.repo")}</span>{" "}
        {skill.repository_url === "local" ? (
          <span>{skill.repository_url}</span>
        ) : (
          <a
            href={skill.repository_url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-500 hover:text-blue-600 transition-colors break-all"
          >
            {skill.repository_url}
          </a>
        )}
      </div>

      {/* Installed Paths */}
      {skill.local_paths && skill.local_paths.length > 0 && (
        <div className="pt-4 border-t border-border/60">
          <div className="text-xs font-medium text-blue-500 mb-3">
            {t("skills.installedPaths")} ({skill.local_paths.length})
          </div>
          <div className="space-y-2">
            {skill.local_paths.map((path, idx) => (
              <div
                key={idx}
                className="flex items-center justify-between gap-3 p-3 bg-secondary/50 rounded-xl"
              >
                <div className="flex items-center gap-3 flex-1 min-w-0">
                  <button
                    onClick={async () => {
                      try {
                        try {
                          await invoke("open_skill_directory", { localPath: path });
                        } catch {
                          await openPath(path);
                        }
                        appToast.success(t("skills.folder.opened"), { duration: 5000 });
                      } catch (error: any) {
                        appToast.error(
                          t("skills.folder.openFailed", { error: error?.message || String(error) }),
                          { duration: 5000 }
                        );
                      }
                    }}
                    aria-label={`${t("skills.folder.opened")}: ${path}`}
                    title={`${t("skills.folder.opened")}: ${path}`}
                    className="text-blue-500 hover:text-blue-600 transition-colors"
                  >
                    <FolderOpen className="w-4 h-4" />
                  </button>
                  <span className="text-sm text-muted-foreground truncate" title={path}>
                    {path}
                  </span>
                </div>
                <button
                  onClick={() => onUninstallPath(path)}
                  disabled={isAnyOperationPending}
                  aria-label={`${t("skills.uninstall")}: ${path}`}
                  title={`${t("skills.uninstall")}: ${path}`}
                  className="text-red-500 hover:text-red-600 transition-colors disabled:opacity-50"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

interface InstalledPluginCardProps {
  plugin: Plugin;
  hasUpdate: boolean;
  latestVersion?: string;
  isUpdating: boolean;
  isUninstalling: boolean;
  isAnyOperationPending: boolean;
  onUpdate: () => void;
  onUninstall: () => void;
}

function InstalledPluginCard({
  plugin,
  hasUpdate,
  latestVersion,
  isUpdating,
  isUninstalling,
  isAnyOperationPending,
  onUpdate,
  onUninstall,
}: InstalledPluginCardProps) {
  const { t } = useTranslation();
  const installPath = plugin.claude_install_path?.trim();
  const descriptionRef = useRef<HTMLParagraphElement | null>(null);
  const [isDescriptionTruncated, setIsDescriptionTruncated] = useState(false);

  useLayoutEffect(() => {
    const element = descriptionRef.current;
    if (!element) return;

    const update = () => {
      setIsDescriptionTruncated(
        element.scrollHeight > element.clientHeight || element.scrollWidth > element.clientWidth
      );
    };

    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [plugin.description]);

  return (
    <div className="apple-card p-6 pt-10 group flex flex-col h-full relative">
      <CardCornerBadge kind="plugin" label={t("plugins.badge")} />
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1">
          <div className="flex items-center gap-2.5 mb-1 flex-wrap">
            <h3 className="font-semibold text-foreground">{plugin.name}</h3>
            <span
              className={`text-xs px-2.5 py-1 rounded-full font-medium ${
                plugin.repository_owner === "local"
                  ? "text-muted-foreground bg-secondary"
                  : "text-blue-600 bg-blue-500/10"
              }`}
            >
              {formatRepositoryTag(plugin)}
            </span>
          </div>
          {plugin.marketplace_name && (
            <div className="text-xs text-muted-foreground">
              {t("plugins.marketplace")}: {plugin.marketplace_name}
            </div>
          )}
        </div>

        <div className="flex gap-2 ml-4">
          {hasUpdate && (
            <button
              onClick={onUpdate}
              disabled={isAnyOperationPending}
              aria-label={`${t("plugins.updates.update")}: ${plugin.name}`}
              title={
                latestVersion
                  ? `${t("plugins.updates.update")}: ${plugin.name} (${t("plugins.updates.available", { version: latestVersion })})`
                  : undefined
              }
              className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5"
            >
              {isUpdating ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  {t("plugins.updates.updating")}
                </>
              ) : (
                <>
                  <Download className="w-3.5 h-3.5" />
                  {t("plugins.updates.update")}
                </>
              )}
            </button>
          )}
          <button
            onClick={onUninstall}
            disabled={isAnyOperationPending}
            aria-label={`${t("plugins.uninstall")}: ${plugin.name}`}
            title={`${t("plugins.uninstall")}: ${plugin.name}`}
            className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
          >
            {isUninstalling ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {t("plugins.uninstalling")}
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5" />
                {t("plugins.uninstall")}
              </>
            )}
          </button>
        </div>
      </div>

      <div className="relative mb-4">
        <p
          ref={descriptionRef}
          title={isDescriptionTruncated && plugin.description ? plugin.description : undefined}
          className="text-sm text-muted-foreground leading-5 h-[3.75rem] overflow-hidden [display:-webkit-box] [-webkit-line-clamp:3] [-webkit-box-orient:vertical] peer"
        >
          {plugin.description || t("plugins.noDescription")}
        </p>
        {isDescriptionTruncated && plugin.description && (
          <div className="apple-tooltip peer-hover:opacity-100 peer-hover:translate-y-0">
            {plugin.description}
          </div>
        )}
      </div>

      <div className="text-sm text-muted-foreground mb-4">
        <span className="text-blue-500 font-medium">{t("plugins.repo")}</span>{" "}
        <a
          href={plugin.repository_url}
          target="_blank"
          rel="noopener noreferrer"
          className="text-blue-500 hover:text-blue-600 transition-colors break-all"
        >
          {plugin.repository_url}
        </a>
      </div>

      {installPath && (
        <div className="pt-4 border-t border-border/60">
          <div className="text-xs font-medium text-blue-500 mb-3">
            {t("skills.installedPaths")} (1)
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-3 p-3 bg-secondary/50 rounded-xl">
              <button
                onClick={async () => {
                  try {
                    try {
                      await invoke("open_skill_directory", { localPath: installPath });
                    } catch {
                      await openPath(installPath);
                    }
                    appToast.success(t("skills.folder.opened"), { duration: 5000 });
                  } catch (error: any) {
                    appToast.error(
                      t("skills.folder.openFailed", { error: error?.message || String(error) }),
                      { duration: 5000 }
                    );
                  }
                }}
                aria-label={`${t("skills.folder.opened")}: ${installPath}`}
                title={`${t("skills.folder.opened")}: ${installPath}`}
                className="text-blue-500 hover:text-blue-600 transition-colors"
              >
                <FolderOpen className="w-4 h-4" />
              </button>
              <span className="text-sm text-muted-foreground truncate" title={installPath}>
                {installPath}
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface UpdateConfirmDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: (forceOverwrite: boolean) => void;
  isConfirming: boolean;
  report: SecurityReport | null;
  conflicts: string[];
  skillName: string;
}

function UpdateConfirmDialog({
  open,
  onClose,
  onConfirm,
  isConfirming,
  report,
  conflicts,
  skillName,
}: UpdateConfirmDialogProps) {
  const { t } = useTranslation();
  const [forceOverwrite, setForceOverwrite] = useState(false);
  const hasConflicts = conflicts.length > 0;
  const confirmTone = !report
    ? "primary"
    : report.score < 50 || report.blocked
      ? "destructive"
      : report.partial_scan || report.score < 70
        ? "warning"
        : "primary";

  return (
    <SkillSecurityDialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
      title={t("skills.installedPage.updateScanResult")}
      skillName={skillName}
      preparingLabel={t("skills.installedPage.preparingUpdate")}
      report={report}
      issuePreviewCount={5}
      contentClassName="max-w-2xl max-h-[80vh] overflow-y-auto"
      leadContent={
        <>
          <div className="rounded-lg bg-primary/10 p-3">
            <div className="flex items-center gap-2 text-sm text-primary">
              <Lightbulb className="h-4 w-4" />
              {t("skills.installedPage.updateTip")}
            </div>
          </div>

          {hasConflicts && (
            <div className="rounded-lg border border-warning/30 bg-warning/10 p-4">
              <div className="mb-1 font-medium text-warning">
                {t("skills.installedPage.conflictDetected")}
              </div>
              <div className="mb-2 text-sm text-muted-foreground">
                {t("skills.installedPage.conflictDescription")}
              </div>
              <ul className="max-h-32 space-y-1 overflow-y-auto text-xs text-warning">
                {conflicts.slice(0, 10).map((conflict, idx) => (
                  <li key={idx}>• {conflict}</li>
                ))}
                {conflicts.length > 10 && (
                  <li className="text-muted-foreground">
                    ... {t("skills.installedPage.andMore", { count: conflicts.length - 10 })}
                  </li>
                )}
              </ul>
              <label className="mt-3 flex cursor-pointer items-center gap-2 rounded-lg bg-card p-2">
                <input
                  type="checkbox"
                  checked={forceOverwrite}
                  onChange={(event) => setForceOverwrite(event.target.checked)}
                  className="h-4 w-4 rounded"
                />
                <span className="text-sm">{t("skills.installedPage.forceOverwrite")}</span>
              </label>
            </div>
          )}
        </>
      }
      footer={
        <>
          <AlertDialogCancel onClick={onClose} disabled={isConfirming}>
            {t("skills.marketplace.install.cancel")}
          </AlertDialogCancel>
          <SkillSecurityDialogConfirmButton
            onClick={() => onConfirm(forceOverwrite)}
            disabled={Boolean(report?.blocked) || (hasConflicts && !forceOverwrite)}
            isLoading={isConfirming}
            loadingLabel={t("skills.installedPage.applyingUpdate")}
            label={
              report?.partial_scan
                ? t("skills.installedPage.continueUpdate")
                : t("skills.marketplace.install.continue")
            }
            tone={confirmTone}
          />
        </>
      }
    />
  );
}
