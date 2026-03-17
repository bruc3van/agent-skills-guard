import { useEffect, useMemo, useRef, useState } from "react";
import {
  useRepositories,
  useAddRepository,
  useDeleteRepository,
  useScanRepository,
} from "../hooks/useRepositories";
import {
  Search,
  Plus,
  Trash2,
  GitBranch,
  Loader2,
  Database,
  ShoppingCart,
  X,
  RefreshCw,
  Download,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";
import { FeaturedRepositories } from "./FeaturedRepositories";
import type { FeaturedMarketplacesConfig, Skill } from "../types";
import type { SecurityReport } from "../types/security";
import { invoke } from "@tauri-apps/api/core";
import { InstallPathSelector } from "./InstallPathSelector";
import { addRecentInstallPath } from "@/lib/storage";
import { formatAppDate, formatAppDateTime } from "@/lib/locale";
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

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

function formatDate(
  dateStr: string,
  language: string,
  t: (key: string, options?: any) => string
): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const days = Math.floor(diff / (1000 * 60 * 60 * 24));

  if (days === 0) return t("repositories.date.today");
  if (days === 1) return t("repositories.date.yesterday");
  if (days < 7) return t("repositories.date.daysAgo", { days });

  return formatAppDate(date, language);
}

function hasFeaturedMarketplacesChanged(
  previous: FeaturedMarketplacesConfig | undefined,
  next: FeaturedMarketplacesConfig
): boolean {
  if (!previous) return true;
  try {
    return JSON.stringify(previous.marketplace) !== JSON.stringify(next.marketplace);
  } catch {
    return true;
  }
}

interface RepositoriesPageProps {
  onNavigateToMarket?: (options?: { marketplaceName?: string }) => void;
}

export function RepositoriesPage({ onNavigateToMarket }: RepositoriesPageProps) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { data: repositories, isLoading } = useRepositories();
  const addMutation = useAddRepository();
  const deleteMutation = useDeleteRepository();
  const scanMutation = useScanRepository();

  const [activeTab, setActiveTab] = useState<
    "featuredMarketplaces" | "featured" | "my"
  >("featuredMarketplaces");

  const [isAddingFeatured, setIsAddingFeatured] = useState(false);
  const [addingFeaturedUrl, setAddingFeaturedUrl] = useState<string | null>(null);

  const [showAddForm, setShowAddForm] = useState(false);
  const [newRepoUrl, setNewRepoUrl] = useState("");
  const [newRepoName, setNewRepoName] = useState("");
  const [scanningRepoId, setScanningRepoId] = useState<string | null>(null);
  const [refreshingRepoId, setRefreshingRepoId] = useState<string | null>(null);
  const [deletingRepoId, setDeletingRepoId] = useState<string | null>(null);
  const addFormRef = useRef<HTMLDivElement | null>(null);
  const urlInputRef = useRef<HTMLInputElement | null>(null);

  const [preview, setPreview] = useState<{
    repoName: string;
    repoUrl: string;
    skills: Skill[];
  } | null>(null);

  const [pendingSkillInstall, setPendingSkillInstall] = useState<{
    skill: Skill;
    report: SecurityReport;
  } | null>(null);
  const [preparingSkillId, setPreparingSkillId] = useState<string | null>(null);
  const [installingSkillId, setInstallingSkillId] = useState<string | null>(null);

  const { data: cacheStats } = useQuery({
    queryKey: ["cache-stats"],
    queryFn: api.getCacheStats,
    refetchInterval: 30000,
  });

  const refreshFeaturedMutation = useMutation({
    mutationFn: api.refreshFeaturedRepositories,
    onSuccess: (data) => {
      queryClient.setQueryData(["featured-repositories"], data);
      appToast.success(t("repositories.featured.refreshed"));
    },
    onError: (error: any) => {
      appToast.error(
        t("repositories.featured.refreshFailed", {
          error: error?.message || String(error),
        })
      );
    },
  });

  const { data: featuredMarketplaces, isLoading: isFeaturedMarketplacesLoading } = useQuery({
    queryKey: ["featured-marketplaces"],
    queryFn: api.getFeaturedMarketplaces,
    staleTime: 5 * 60 * 1000,
    retry: false,
  });

  const refreshFeaturedMarketplacesMutation = useMutation({
    mutationFn: api.refreshFeaturedMarketplaces,
    onSuccess: async (data) => {
      const previousConfig = queryClient.getQueryData<FeaturedMarketplacesConfig>([
        "featured-marketplaces",
      ]);
      queryClient.setQueryData(["featured-marketplaces"], data);
      appToast.success(t("repositories.featuredMarketplaces.refreshed"));
      if (hasFeaturedMarketplacesChanged(previousConfig, data)) {
        try {
          const plugins = await api.syncFeaturedMarketplacePlugins(i18n.language);
          queryClient.setQueryData(["plugins", i18n.language], plugins);
        } catch (error) {
          console.debug(
            "Failed to refresh plugins after featured marketplaces update:",
            error
          );
        }
      }
    },
    onError: (error: any) => {
      appToast.error(
        t("repositories.featuredMarketplaces.refreshFailed", {
          error: error?.message || String(error),
        })
      );
    },
  });

  const refreshCacheMutation = useMutation({
    mutationFn: api.refreshRepositoryCache,
    onSuccess: (skills) => {
      queryClient.invalidateQueries({ queryKey: ["repositories"] });
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["plugins"] });
      appToast.success(t("repositories.cache.refreshed", { count: skills.length }));
    },
    onError: (error: any) => {
      appToast.error(t("repositories.cache.refreshFailed", { error: error.message || error }));
    },
  });

  const clearAllCachesMutation = useMutation({
    mutationFn: api.clearAllRepositoryCaches,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ["repositories"] });
      queryClient.invalidateQueries({ queryKey: ["cache-stats"] });
      appToast.success(
        t("repositories.cache.clearedAll", {
          cleared: result.clearedCount,
          failed: result.failedCount,
          size: formatBytes(result.totalSizeFreed),
        })
      );
    },
    onError: (error: any) => {
      appToast.error(t("repositories.cache.clearAllFailed", { error: error.message || error }));
    },
  });

  const extractRepoNameFromUrl = (url: string): string => {
    try {
      const match = url.match(/github\.com[:/]([^/]+)\//);
      if (match && match[1]) {
        return match[1];
      }
      return "";
    } catch {
      return "";
    }
  };

  const handleUrlChange = (url: string) => {
    setNewRepoUrl(url);
    if (!newRepoName) {
      const extracted = extractRepoNameFromUrl(url);
      if (extracted) {
        setNewRepoName(extracted);
      }
    }
  };

  const getLocalizedText = (text: { en: string; zh: string }) => {
    return i18n.language === "zh" ? text.zh : text.en;
  };

  const openRepositoryPreview = async (repoUrl: string, repoName: string) => {
    try {
      const [skills] = await Promise.all([api.getSkills()]);
      const repoSkills = skills.filter(
        (skill) => skill.repository_url === repoUrl && skill.repository_owner !== "local"
      );
      setPreview({ repoName, repoUrl, skills: repoSkills });
    } catch (error: any) {
      appToast.error(
        t("repositories.preview.loadFailed", { error: error?.message || String(error) })
      );
    }
  };

  const prepareSkillInstall = async (skill: Skill) => {
    if (preparingSkillId || installingSkillId) return;
    try {
      setPreparingSkillId(skill.id);
      const report = await invoke<SecurityReport>("prepare_skill_installation", {
        skillId: skill.id,
        locale: i18n.language,
      });
      setPendingSkillInstall({ skill, report });
    } catch (error: any) {
      appToast.error(`${t("skills.toast.installFailed")}: ${error.message || error}`);
    } finally {
      setPreparingSkillId(null);
    }
  };

  const handleAddRepository = () => {
    if (newRepoUrl && newRepoName) {
      addMutation.mutate(
        { url: newRepoUrl, name: newRepoName },
        {
          onSuccess: (repoId) => {
            const repoUrl = newRepoUrl;
            const repoName = newRepoName;
            setNewRepoUrl("");
            setNewRepoName("");
            setShowAddForm(false);
            appToast.success(t("repositories.toast.added"));

            setScanningRepoId(repoId);
            scanMutation.mutate(repoId, {
              onSuccess: (skills) => {
                setScanningRepoId(null);
                appToast.success(t("repositories.toast.foundSkills", { count: skills.length }));
                void openRepositoryPreview(repoUrl, repoName);
              },
              onError: (error: any) => {
                setScanningRepoId(null);
                appToast.error(`${t("repositories.toast.scanError")}${error.message || error}`);
              },
            });
          },
          onError: (error: any) => {
            appToast.error(`${t("repositories.toast.error")}${error.message || error}`);
          },
        }
      );
    }
  };

  useEffect(() => {
    if (!showAddForm || activeTab !== "my") return;
    requestAnimationFrame(() => {
      addFormRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
      urlInputRef.current?.focus();
    });
  }, [activeTab, showAddForm]);

  const pageBusyMessage = useMemo(() => {
    if (refreshFeaturedMarketplacesMutation.isPending) {
      return t("repositories.busy.refreshFeaturedMarketplaces");
    }
    if (refreshFeaturedMutation.isPending) {
      return t("repositories.busy.refreshFeatured");
    }
    if (refreshCacheMutation.isPending && refreshingRepoId) {
      const repo = repositories?.find((item) => item.id === refreshingRepoId);
      return t("repositories.busy.refreshRepository", { name: repo?.name ?? "" });
    }
    if (scanMutation.isPending && scanningRepoId) {
      const repo = repositories?.find((item) => item.id === scanningRepoId);
      return t("repositories.busy.scanRepository", { name: repo?.name ?? "" });
    }
    if (deleteMutation.isPending && deletingRepoId) {
      const repo = repositories?.find((item) => item.id === deletingRepoId);
      return t("repositories.busy.deleteRepository", { name: repo?.name ?? "" });
    }
    if (isAddingFeatured) {
      return t("repositories.busy.addFeatured");
    }
    if (addMutation.isPending) {
      return t("repositories.busy.addRepository");
    }
    if (preparingSkillId) {
      const skill = preview?.skills.find((item) => item.id === preparingSkillId);
      return t("repositories.busy.prepareSkill", { name: skill?.name ?? "" });
    }
    if (installingSkillId) {
      const skill = preview?.skills.find((item) => item.id === installingSkillId);
      return t("repositories.busy.installSkill", { name: skill?.name ?? "" });
    }
    return null;
  }, [
    addMutation.isPending,
    deleteMutation.isPending,
    deletingRepoId,
    installingSkillId,
    isAddingFeatured,
    preparingSkillId,
    preview?.skills,
    refreshCacheMutation.isPending,
    refreshFeaturedMarketplacesMutation.isPending,
    refreshFeaturedMutation.isPending,
    refreshingRepoId,
    repositories,
    scanMutation.isPending,
    scanningRepoId,
    t,
  ]);

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-headline text-foreground">{t("repositories.title")}</h1>
        </div>
        {activeTab === "my" ? (
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className={`flex items-center gap-2 ${showAddForm ? "apple-button-secondary" : "apple-button-primary"}`}
          >
            {showAddForm ? (
              <>
                <X className="w-4 h-4" />
                {t("repositories.cancel")}
              </>
            ) : (
              <>
                <Plus className="w-4 h-4" />
                {t("repositories.addRepo")}
              </>
            )}
          </button>
        ) : activeTab === "featuredMarketplaces" ? (
          <button
            onClick={() => refreshFeaturedMarketplacesMutation.mutate()}
            disabled={refreshFeaturedMarketplacesMutation.isPending}
            className="flex items-center gap-2 apple-button-primary disabled:opacity-50"
          >
            {refreshFeaturedMarketplacesMutation.isPending ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                {t("repositories.featuredMarketplaces.refreshing")}
              </>
            ) : (
              <>
                <RefreshCw className="w-4 h-4" />
                {t("repositories.featuredMarketplaces.refresh")}
              </>
            )}
          </button>
        ) : (
          <button
            onClick={() => refreshFeaturedMutation.mutate()}
            disabled={refreshFeaturedMutation.isPending}
            className="flex items-center gap-2 apple-button-primary disabled:opacity-50"
          >
            {refreshFeaturedMutation.isPending ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                {t("repositories.featured.refreshing")}
              </>
            ) : (
              <>
                <RefreshCw className="w-4 h-4" />
                {t("repositories.featured.refresh")}
              </>
            )}
          </button>
        )}
      </div>

      <div className="flex items-center gap-2">
        <RepositoriesTabButton
          active={activeTab === "featuredMarketplaces"}
          onClick={() => {
            setActiveTab("featuredMarketplaces");
            setShowAddForm(false);
          }}
          label={t("repositories.tabs.featuredMarketplaces")}
        />
        <RepositoriesTabButton
          active={activeTab === "featured"}
          onClick={() => {
            setActiveTab("featured");
            setShowAddForm(false);
          }}
          label={t("repositories.tabs.featured")}
        />
        <RepositoriesTabButton
          active={activeTab === "my"}
          onClick={() => setActiveTab("my")}
          label={t("repositories.tabs.my", { count: repositories?.length ?? 0 })}
        />
      </div>

      {pageBusyMessage && <PageBusyNotice message={pageBusyMessage} />}

      {activeTab === "featuredMarketplaces" && (
        <div className="space-y-6">
          <div className="apple-card p-5">
            <div className="flex items-center gap-2 mb-4">
              <ShoppingCart className="w-5 h-5 text-warning" />
              <h3 className="font-medium">{t("repositories.featuredMarketplaces.title")}</h3>
            </div>

            {isFeaturedMarketplacesLoading ? (
              <div className="text-sm text-muted-foreground">
                {t("repositories.featuredMarketplaces.loading")}
              </div>
            ) : !featuredMarketplaces || featuredMarketplaces.marketplace.length === 0 ? (
              <div className="text-sm text-muted-foreground">
                {t("repositories.featuredMarketplaces.empty")}
              </div>
            ) : (
              <div className="space-y-5">
                {featuredMarketplaces.marketplace.map((category) => (
                  <div key={category.id} className="space-y-3">
                    <div>
                      <h4 className="font-semibold text-foreground">
                        {getLocalizedText(category.name)}
                      </h4>
                      <p className="text-sm text-muted-foreground">
                        {getLocalizedText(category.description)}
                      </p>
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      {category.marketplaces.map((marketplace) => (
                        <div
                          key={marketplace.marketplace_name}
                          className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between p-3 bg-card rounded-lg border border-border hover:border-primary/30 transition-all"
                        >
                          <div className="flex-1">
                            <div className="flex items-center gap-2 mb-1">
                              <h5 className="text-sm font-medium text-primary">
                                {marketplace.marketplace_name}
                              </h5>
                            </div>
                            <p className="text-xs text-muted-foreground mb-2 overflow-hidden [display:-webkit-box] [-webkit-line-clamp:3] [-webkit-box-orient:vertical]">
                              {getLocalizedText(marketplace.description)}
                            </p>
                            {marketplace.repository_url && (
                              <a
                                href={marketplace.repository_url}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="text-xs text-blue-500 hover:text-blue-600 hover:underline break-all transition-colors"
                              >
                                {marketplace.repository_url}
                              </a>
                            )}
                            <div className="text-xs text-muted-foreground mt-2">
                              {t("repositories.featuredMarketplaces.pluginsCount", {
                                count: marketplace.plugins.length,
                              })}
                            </div>
                            <div className="flex flex-wrap gap-1 mt-2">
                              {marketplace.tags.map((tag) => (
                                <span
                                  key={tag}
                                  className="px-2 py-0.5 text-xs bg-primary/10 text-primary rounded-full"
                                >
                                  {tag}
                                </span>
                              ))}
                            </div>
                          </div>

                          <button
                            onClick={() =>
                              onNavigateToMarket?.({
                                marketplaceName: marketplace.marketplace_name,
                              })
                            }
                            disabled={!onNavigateToMarket}
                            className="self-end sm:self-auto sm:ml-4 text-xs flex items-center gap-1.5 disabled:opacity-50 macos-button-primary"
                          >
                            {t("repositories.featuredMarketplaces.viewMarketplace")}
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {activeTab === "featured" && (
        <div className="space-y-6">
          <FeaturedRepositories
            variant="page"
            layout="expanded"
            showHeader={false}
            categoryIds={["official", "community"]}
            onAdd={async (url, name) => {
              if (isAddingFeatured) return;

              setIsAddingFeatured(true);
              setAddingFeaturedUrl(url);

              try {
                const repoId = await addMutation.mutateAsync({ url, name });
                appToast.success(t("repositories.toast.added"));

                try {
                  const skills = await scanMutation.mutateAsync(repoId);
                  appToast.success(t("repositories.toast.foundSkills", { count: skills.length }));
                } catch (error: any) {
                  appToast.error(`${t("repositories.toast.scanError")}${error.message || error}`);
                }

                void openRepositoryPreview(url, name);
              } catch (error: any) {
                appToast.error(`${t("repositories.toast.error")}${error.message || error}`);
              } finally {
                setIsAddingFeatured(false);
                setAddingFeaturedUrl(null);
              }
            }}
            isAdding={isAddingFeatured}
            addingUrl={addingFeaturedUrl}
          />
        </div>
      )}

      {/* Cache Statistics */}
      {activeTab === "my" && cacheStats && (
        <div className="apple-card p-5">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2">
              <Database className="w-4 h-4 text-blue-500" />
              <h3 className="font-semibold text-sm">{t("repositories.cache.stats")}</h3>
            </div>

            {cacheStats.cachedRepositories > 0 && (
              <button
                onClick={() => clearAllCachesMutation.mutate()}
                disabled={clearAllCachesMutation.isPending}
                className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
              >
                {clearAllCachesMutation.isPending ? (
                  <>
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    {t("repositories.cache.clearing")}
                  </>
                ) : (
                  <>
                    <Trash2 className="w-3.5 h-3.5" />
                    {t("repositories.cache.clearAll")}
                  </>
                )}
              </button>
            )}
          </div>

          <div className="grid grid-cols-3 gap-4">
            <div className="p-4 bg-secondary/50 rounded-xl">
              <div className="text-xs text-muted-foreground mb-1">
                {t("repositories.cache.totalRepos")}
              </div>
              <div className="text-2xl font-semibold text-blue-500">
                {cacheStats.totalRepositories}
              </div>
            </div>

            <div className="p-4 bg-secondary/50 rounded-xl">
              <div className="text-xs text-muted-foreground mb-1">
                {t("repositories.cache.cached")}
              </div>
              <div className="text-2xl font-semibold text-green-600">
                {cacheStats.cachedRepositories}
              </div>
            </div>

            <div className="p-4 bg-secondary/50 rounded-xl">
              <div className="text-xs text-muted-foreground mb-1">
                {t("repositories.cache.size")}
              </div>
              <div className="text-2xl font-semibold text-purple-500">
                {formatBytes(cacheStats.totalSizeBytes)}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Add Repository Form */}
      {activeTab === "my" && showAddForm && (
        <div ref={addFormRef} className="apple-card p-6">
          <div className="flex items-center gap-3 mb-5">
            <div className="w-9 h-9 rounded-xl bg-blue-500 flex items-center justify-center">
              <GitBranch className="w-4 h-4 text-white" />
            </div>
            <h3 className="font-semibold">{t("repositories.newRepository")}</h3>
          </div>

          <div className="space-y-5">
            <div>
              <label className="block text-sm font-medium text-foreground mb-2">
                {t("repositories.githubUrl")}
              </label>
              <input
                type="text"
                value={newRepoUrl}
                onChange={(e) => handleUrlChange(e.target.value)}
                placeholder="https://github.com/owner/repo"
                className="apple-input w-full"
                ref={urlInputRef}
              />
              <p className="text-xs text-muted-foreground mt-2">{t("repositories.urlHint")}</p>
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-2">
                {t("repositories.repoName")}
              </label>
              <input
                type="text"
                value={newRepoName}
                onChange={(e) => setNewRepoName(e.target.value)}
                placeholder="owner"
                className="apple-input w-full"
              />
              <p className="text-xs text-muted-foreground mt-2">{t("repositories.nameHint")}</p>
            </div>
          </div>

          <div className="flex gap-3 mt-6">
            <button
              onClick={handleAddRepository}
              className="apple-button-primary flex-1 flex items-center justify-center gap-2 disabled:opacity-50"
              disabled={!newRepoUrl || !newRepoName || addMutation.isPending}
            >
              {addMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  {t("repositories.adding")}
                </>
              ) : (
                <>
                  <Plus className="w-4 h-4" />
                  {t("repositories.confirmAdd")}
                </>
              )}
            </button>
            <button
              onClick={() => {
                setShowAddForm(false);
                setNewRepoUrl("");
                setNewRepoName("");
              }}
              className="apple-button-secondary"
              disabled={addMutation.isPending}
            >
              {t("repositories.cancel")}
            </button>
          </div>
        </div>
      )}

      {/* Repository List */}
      {activeTab === "my" &&
        (isLoading ? (
          <div className="flex flex-col items-center justify-center py-20">
            <Loader2 className="w-10 h-10 text-blue-500 animate-spin mb-4" />
            <p className="text-sm text-muted-foreground">{t("repositories.loading")}</p>
          </div>
        ) : repositories && repositories.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
            {repositories.map((repo) => (
              <div key={repo.id} className="apple-card p-6">
                <div className="flex flex-col gap-4">
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2.5 mb-2">
                        <GitBranch className="w-4 h-4 text-blue-500" />
                        <h3 className="font-semibold text-foreground">{repo.name}</h3>
                      </div>

                      <div className="text-sm text-muted-foreground mb-2 pl-6">
                        <span className="text-blue-500">{t("repositories.url")}</span>{" "}
                        <span className="break-all">{repo.url}</span>
                      </div>

                      {repo.description && (
                        <p className="text-sm text-muted-foreground pl-6">{repo.description}</p>
                      )}
                    </div>

                    {/* Action Buttons */}
                    <div className="flex gap-2 flex-shrink-0">
                      <button
                        onClick={() => {
                          if (repo.cache_path) {
                            setRefreshingRepoId(repo.id);
                            refreshCacheMutation.mutate(repo.id, {
                              onSuccess: (skills) => {
                                setRefreshingRepoId(null);
                                appToast.success(
                                  t("repositories.toast.foundSkills", { count: skills.length })
                                );
                              },
                              onError: (error: any) => {
                                setRefreshingRepoId(null);
                                appToast.error(
                                  `${t("repositories.toast.scanError")}${error.message || error}`
                                );
                              },
                            });
                          } else {
                            setScanningRepoId(repo.id);
                            scanMutation.mutate(repo.id, {
                              onSuccess: (skills) => {
                                setScanningRepoId(null);
                                appToast.success(
                                  t("repositories.toast.foundSkills", { count: skills.length })
                                );
                                void openRepositoryPreview(repo.url, repo.name);
                              },
                              onError: (error: any) => {
                                setScanningRepoId(null);
                                appToast.error(
                                  `${t("repositories.toast.scanError")}${error.message || error}`
                                );
                              },
                            });
                          }
                        }}
                        disabled={
                          scanMutation.isPending ||
                          refreshCacheMutation.isPending ||
                          deleteMutation.isPending
                        }
                        aria-label={`${repo.cache_path ? t("repositories.rescan") : t("repositories.scan")}: ${repo.name}`}
                        title={`${repo.cache_path ? t("repositories.rescan") : t("repositories.scan")}: ${repo.name}`}
                        className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
                      >
                        {(scanningRepoId === repo.id && scanMutation.isPending) ||
                        (refreshingRepoId === repo.id && refreshCacheMutation.isPending) ? (
                          <>
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                            <span className="hidden sm:inline">
                              {repo.cache_path
                                ? t("repositories.rescanning")
                                : t("repositories.scanning")}
                            </span>
                          </>
                        ) : (
                          <>
                            {repo.cache_path ? (
                              <RefreshCw className="w-3.5 h-3.5" />
                            ) : (
                              <Search className="w-3.5 h-3.5" />
                            )}
                            <span className="hidden sm:inline">
                              {repo.cache_path ? t("repositories.rescan") : t("repositories.scan")}
                            </span>
                          </>
                        )}
                      </button>

                      <button
                        onClick={() => {
                          setDeletingRepoId(repo.id);
                          deleteMutation.mutate(repo.id, {
                            onSuccess: () => setDeletingRepoId(null),
                            onError: () => setDeletingRepoId(null),
                          });
                        }}
                        disabled={
                          scanMutation.isPending ||
                          refreshCacheMutation.isPending ||
                          deleteMutation.isPending
                        }
                        aria-label={`${t("common.delete")}: ${repo.name}`}
                        title={`${t("common.delete")}: ${repo.name}`}
                        className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50"
                      >
                        {deletingRepoId === repo.id ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="w-3.5 h-3.5" />
                        )}
                      </button>
                    </div>
                  </div>

                  {/* Metadata */}
                  <div className="flex flex-wrap items-center gap-4 pl-6 text-xs">
                    {repo.last_scanned && (
                      <div className="text-muted-foreground">
                        <span className="text-blue-500 font-medium">
                          {t("repositories.lastScan")}
                        </span>{" "}
                        {formatAppDateTime(repo.last_scanned, i18n.language)}
                      </div>
                    )}

                    <div>
                      {repo.cache_path ? (
                        <span className="inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium bg-green-500/10 text-green-600">
                          {t("repositories.cache.statusCached")}
                          {repo.cached_at && ` · ${formatDate(repo.cached_at, i18n.language, t)}`}
                        </span>
                      ) : (
                        <span className="inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium bg-secondary text-muted-foreground">
                          {t("repositories.cache.statusUncached")}
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="apple-card p-16 text-center">
            <div className="w-20 h-20 rounded-full bg-secondary flex items-center justify-center mx-auto mb-5">
              <Database className="w-10 h-10 text-muted-foreground" />
            </div>
            <p className="text-sm text-muted-foreground mb-2">{t("repositories.noReposFound")}</p>
            <p className="text-xs text-muted-foreground">{t("repositories.clickAddRepo")}</p>
          </div>
        ))}

      <AlertDialog open={preview !== null} onOpenChange={(open) => !open && setPreview(null)}>
        <AlertDialogContent className="max-w-3xl max-h-[85vh] overflow-y-auto">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("repositories.preview.title", { name: preview?.repoName || "" })}
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-4">
                <div className="text-sm text-muted-foreground break-all">{preview?.repoUrl}</div>

                <div className="p-3 rounded-lg bg-muted/40 border border-border/60">
                  <div className="text-sm font-medium mb-2">
                    {t("repositories.preview.foundTitle")}
                  </div>
                  <div className="text-sm text-muted-foreground">
                    {t("repositories.preview.foundSummary", {
                      skills: preview?.skills.length || 0,
                    })}
                  </div>
                </div>

                <div className="space-y-2">
                  {(preview?.skills || []).map((skill) => (
                    <div
                      key={skill.id}
                      className="flex items-center justify-between gap-3 p-3 rounded-xl bg-card border border-border/60"
                    >
                      <div className="min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-xs px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-600">
                            {t("skills.badge")}
                          </span>
                          <span className="font-medium text-foreground truncate">{skill.name}</span>
                        </div>
                        <div className="text-xs text-muted-foreground mt-1 overflow-hidden [display:-webkit-box] [-webkit-line-clamp:2] [-webkit-box-orient:vertical]">
                          {skill.description || t("skills.noDescription")}
                        </div>
                      </div>

                      <button
                        onClick={() => prepareSkillInstall(skill)}
                        disabled={
                          skill.installed ||
                          preparingSkillId !== null ||
                          installingSkillId !== null
                        }
                        className="apple-button-primary h-8 px-3 text-xs flex items-center gap-1.5 disabled:opacity-50 flex-shrink-0"
                      >
                        {skill.installed ? (
                          t("market.installed")
                        ) : preparingSkillId === skill.id ? (
                          <>
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                            {t("skills.scanning")}
                          </>
                        ) : (
                          <>
                            <Download className="w-3.5 h-3.5" />
                            {t("skills.install")}
                          </>
                        )}
                      </button>
                    </div>
                  ))}

                </div>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>

          <AlertDialogFooter>
            <button
              type="button"
              onClick={() => {
                setPreview(null);
                onNavigateToMarket?.();
              }}
              disabled={!onNavigateToMarket}
              className="apple-button-secondary disabled:opacity-50"
            >
              {t("repositories.preview.goToMarket")}
            </button>
            <AlertDialogCancel>{t("repositories.preview.close")}</AlertDialogCancel>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <SkillInstallConfirmDialog
        open={pendingSkillInstall !== null}
        report={pendingSkillInstall?.report || null}
        skillName={pendingSkillInstall?.skill.name || ""}
        onClose={() => {
          const skillId = pendingSkillInstall?.skill.id;
          const shouldCancel = skillId && installingSkillId !== skillId;
          setPendingSkillInstall(null);
          if (!shouldCancel) return;
          void invoke("cancel_skill_installation", { skillId }).catch((error: any) => {
            console.error("[ERROR] 取消安装失败:", error);
          });
        }}
        onConfirm={async (selectedPath) => {
          if (!pendingSkillInstall) return;
          const skillId = pendingSkillInstall.skill.id;
          setInstallingSkillId(skillId);
          setPendingSkillInstall(null);
          try {
            await invoke("confirm_skill_installation", {
              skillId,
              installPath: selectedPath,
              allowPartialScan: Boolean(
                pendingSkillInstall.report?.partial_scan ||
                  pendingSkillInstall.report?.skipped_files?.length
              ),
            });
            addRecentInstallPath(selectedPath);
            await queryClient.refetchQueries({ queryKey: ["skills"] });
            await queryClient.refetchQueries({ queryKey: ["skills", "installed"] });
            await queryClient.refetchQueries({ queryKey: ["scanResults"] });
            setPreview((prev) => {
              if (!prev) return prev;
              return {
                ...prev,
                skills: prev.skills.map((skill) =>
                  skill.id === skillId ? { ...skill, installed: true } : skill
                ),
              };
            });
            appToast.success(t("skills.toast.installed"));
          } catch (error: any) {
            appToast.error(`${t("skills.toast.installFailed")}: ${error.message || error}`);
          } finally {
            setInstallingSkillId(null);
          }
        }}
      />
    </div>
  );
}

function RepositoriesTabButton({
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

function SkillInstallConfirmDialog({
  open,
  report,
  skillName,
  onClose,
  onConfirm,
}: {
  open: boolean;
  report: SecurityReport | null;
  skillName: string;
  onClose: () => void;
  onConfirm: (selectedPath: string) => void;
}) {
  const { t } = useTranslation();
  const [selectedPath, setSelectedPath] = useState<string>("");

  useEffect(() => {
    if (!open) setSelectedPath("");
  }, [open]);

  const confirmTone = !report
    ? "primary"
    : report.score < 50 || report.blocked
      ? "destructive"
      : report.partial_scan || report.score < 70
        ? "warning"
        : "success";

  return (
    <SkillSecurityDialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
      title={t("skills.marketplace.install.scanResult")}
      skillName={skillName}
      preparingLabel={t("skills.marketplace.install.preparingInstall")}
      report={report}
      issuePreviewCount={3}
      contentClassName="max-w-2xl max-h-[80vh] overflow-y-auto"
      extraContent={
        <div className="border-t border-border py-4">
          <InstallPathSelector onSelect={setSelectedPath} />
        </div>
      }
      footer={
        <>
          <AlertDialogCancel onClick={onClose}>
            {t("skills.marketplace.install.cancel")}
          </AlertDialogCancel>
          <SkillSecurityDialogConfirmButton
            onClick={() => onConfirm(selectedPath)}
            disabled={!selectedPath}
            loadingLabel={t("skills.installing")}
            label={
              report?.partial_scan
                ? t("skills.marketplace.install.installCautiously")
                : report && (report.score < 50 || report.blocked)
                ? t("skills.marketplace.install.installAnyway")
                : t("skills.marketplace.install.confirmInstall")
            }
            tone={confirmTone}
          />
        </>
      }
    />
  );
}
