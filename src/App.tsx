import { Suspense, lazy, useState, useEffect, useCallback, useRef } from "react";
import { QueryClient, QueryClientProvider, useQueryClient } from "@tanstack/react-query";
import { Sidebar } from "./components/Sidebar";
import { WindowControls } from "./components/WindowControls";
import { UpdateBadge } from "./components/UpdateBadge";
import { Toaster } from "sonner";
import { getPlatform, type Platform } from "./lib/platform";
import { api } from "./lib/api";
import { appToast } from "./lib/toast";
import appIconUrl from "../app-icon.png";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from "./components/ui/alert-dialog";
import type { TabType } from "./types";
import { pluginsCachedQueryKey } from "./hooks/usePlugins";

const reactQueryClient = new QueryClient();
type MarketplacePreset = { marketplaceName?: string } | null;

const OverviewPage = lazy(() =>
  import("./components/OverviewPage").then((module) => ({ default: module.OverviewPage }))
);
const MarketplacePage = lazy(() =>
  import("./components/MarketplacePage").then((module) => ({ default: module.MarketplacePage }))
);
const InstalledSkillsPage = lazy(() =>
  import("./components/InstalledSkillsPage").then((module) => ({
    default: module.InstalledSkillsPage,
  }))
);
const RepositoriesPage = lazy(() =>
  import("./components/RepositoriesPage").then((module) => ({ default: module.RepositoriesPage }))
);
const SettingsPage = lazy(() =>
  import("./components/SettingsPage").then((module) => ({ default: module.SettingsPage }))
);

const ONBOARDING_IMPORT_FEATURED_KEY = "asguard.onboarding.importFeatured.v1";
import { isThrottleDue, markThrottleCompleted } from "./lib/rateLimit";

const FEATURED_REPOSITORIES_REFRESHED_AT_KEY = "asguard.featuredRepositories.refreshedAt.v1";
const FEATURED_MARKETPLACES_REFRESHED_AT_KEY = "asguard.featuredMarketplaces.refreshedAt.v1";
const FEATURED_RESOURCES_REFRESH_INTERVAL_MS = 6 * 60 * 60 * 1000;

function PageFallback() {
  return (
    <div className="h-full flex items-center justify-center gap-3 text-sm text-muted-foreground">
      <Loader2 className="w-4 h-4 animate-spin" />
      <span>加载中...</span>
    </div>
  );
}

function AppContent() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [currentTab, setCurrentTab] = useState<TabType>("overview");
  const [marketplacePreset, setMarketplacePreset] = useState<MarketplacePreset>(null);
  const [platform, setPlatform] = useState<Platform | null>(null);
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [isImportingFeatured, setIsImportingFeatured] = useState(false);
  const didRunStartupTasksRef = useRef(false);
  const didRunOnboardingCheckRef = useRef(false);
  const langRef = useRef(i18n.language);
  langRef.current = i18n.language;

  const clearMarketplacePreset = useCallback(() => {
    setMarketplacePreset(null);
  }, []);

  useEffect(() => {
    getPlatform().then(setPlatform);
  }, []);

  useEffect(() => {
    if (platform === null) return;
    const className = "macos-window";
    if (platform === "macos") {
      document.body.classList.add(className);
    } else {
      document.body.classList.remove(className);
    }
    return () => {
      document.body.classList.remove(className);
    };
  }, [platform]);

  // 启动时任务（防止 StrictMode 下重复触发）
  useEffect(() => {
    if (didRunStartupTasksRef.current) return;
    didRunStartupTasksRef.current = true;

    let cancelled = false;

    const refreshFeaturedResources = async () => {
      const tasks: Promise<void>[] = [];

      if (
        isThrottleDue(FEATURED_REPOSITORIES_REFRESHED_AT_KEY, FEATURED_RESOURCES_REFRESH_INTERVAL_MS)
      ) {
        tasks.push(
          api
            .refreshFeaturedRepositories()
            .then((data) => {
              if (cancelled) return;
              queryClient.setQueryData(["featured-repositories"], data);
              markThrottleCompleted(FEATURED_REPOSITORIES_REFRESHED_AT_KEY);
            })
            .catch((error) => {
              console.debug("Failed to auto-update featured repositories:", error);
            })
        );
      }

      if (
        isThrottleDue(FEATURED_MARKETPLACES_REFRESHED_AT_KEY, FEATURED_RESOURCES_REFRESH_INTERVAL_MS)
      ) {
        tasks.push(
          api
            .refreshFeaturedMarketplaces()
            .then(async (data) => {
              if (cancelled) return;
              queryClient.setQueryData(["featured-marketplaces"], data);
              markThrottleCompleted(FEATURED_MARKETPLACES_REFRESHED_AT_KEY);

              try {
                const lang = langRef.current;
                const plugins = await api.syncFeaturedMarketplacePlugins(lang);
                if (!cancelled) {
                  queryClient.setQueryData(pluginsCachedQueryKey(lang), plugins);
                }
              } catch (error) {
                console.debug(
                  "Failed to sync featured marketplace plugins after auto-refresh:",
                  error
                );
              }
            })
            .catch((error) => {
              console.debug("Failed to auto-update featured marketplaces:", error);
            })
        );
      }

      await Promise.allSettled(tasks);
    };

    const refreshTimer = setTimeout(() => {
      void refreshFeaturedResources();
    }, 1500);

    // 首次启动时自动扫描未扫描的仓库，稍微延后到启动高峰之后
    const autoScanRepositories = async () => {
      try {
        const scannedRepos = await api.autoScanUnscannedRepositories();
        if (scannedRepos.length > 0) {
          queryClient.invalidateQueries({ queryKey: ["skills"] });
          queryClient.invalidateQueries({ queryKey: ["plugins"] });
          queryClient.invalidateQueries({ queryKey: ["repositories"] });
        }
      } catch (error) {
        console.debug("自动扫描仓库失败:", error);
      }
    };
    const autoScanTimer = setTimeout(autoScanRepositories, 2500);
    return () => {
      cancelled = true;
      clearTimeout(refreshTimer);
      clearTimeout(autoScanTimer);
    };
  }, [queryClient]);

  // 首次进入程序时提示是否导入精选仓库（官方推荐 + 社区精选）
  useEffect(() => {
    if (didRunOnboardingCheckRef.current) return;
    didRunOnboardingCheckRef.current = true;

    let cancelled = false;

    const hasDecision = () => {
      try {
        return localStorage.getItem(ONBOARDING_IMPORT_FEATURED_KEY) !== null;
      } catch {
        return false;
      }
    };

    if (hasDecision()) return;

    (async () => {
      try {
        const repos = await api.getRepositories();
        if (cancelled) return;
        if (repos.length > 0) return;

        try {
          const localSkills = await api.scanLocalSkills();
          if (cancelled) return;
          if (localSkills.length > 0) {
            queryClient.invalidateQueries({ queryKey: ["skills"] });
            queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
            queryClient.invalidateQueries({ queryKey: ["scanResults"] });
            return;
          }
        } catch (error) {
          console.debug("Failed to auto-detect local skills for onboarding:", error);
        }

        try {
          const plugins = await api.getPluginsCached();
          if (cancelled) return;
          if (plugins.some((plugin) => plugin.installed)) return;
        } catch (error) {
          console.debug("Failed to inspect cached plugins for onboarding:", error);
        }

        setImportDialogOpen(true);
      } catch (error) {
        console.debug("Failed to check repositories for onboarding:", error);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [queryClient]);

  const dismissImportDialog = () => {
    if (isImportingFeatured) return;
    try {
      localStorage.setItem(ONBOARDING_IMPORT_FEATURED_KEY, "skipped");
    } catch {
      // ignore
    }
    setImportDialogOpen(false);
  };

  const confirmImportFeatured = async () => {
    if (isImportingFeatured) return;
    setIsImportingFeatured(true);

    try {
      const result = await api.importFeaturedRepositories(["official", "community"]);

      try {
        localStorage.setItem(ONBOARDING_IMPORT_FEATURED_KEY, "imported");
      } catch {
        // ignore
      }

      setImportDialogOpen(false);
      setCurrentTab("marketplace");
      queryClient.invalidateQueries({ queryKey: ["repositories"] });

      if (result.added_count > 0) {
        appToast.success(
          t("onboarding.importFeatured.toast.added", {
            added: result.added_count,
            total: result.total_count,
            skipped: result.skipped_count,
          })
        );
      } else {
        appToast.info(t("onboarding.importFeatured.toast.nothingToAdd"));
      }

      try {
        appToast.info(t("onboarding.importFeatured.toast.scanning"));
        const scannedRepos = await api.autoScanUnscannedRepositories();
        if (scannedRepos.length > 0) {
          queryClient.invalidateQueries({ queryKey: ["skills"] });
          queryClient.invalidateQueries({ queryKey: ["plugins"] });
          queryClient.invalidateQueries({ queryKey: ["repositories"] });
        }
      } catch (error) {
        console.debug("Auto scan after importing featured repositories failed:", error);
      }
    } catch (error: any) {
      appToast.error(
        t("onboarding.importFeatured.toast.failed", {
          error: error?.message || String(error),
        })
      );
    } finally {
      setIsImportingFeatured(false);
    }
  };

  return (
    <div
      className={`h-screen flex flex-col overflow-hidden bg-background ${
        platform === "macos" ? "macos-window-frame" : ""
      }`}
    >
      <AlertDialog
        open={importDialogOpen}
        onOpenChange={(open) => {
          if (!open) dismissImportDialog();
        }}
      >
        <AlertDialogContent className="max-w-xl">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("onboarding.importFeatured.title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("onboarding.importFeatured.description")}
            </AlertDialogDescription>
          </AlertDialogHeader>

          <AlertDialogFooter>
            <AlertDialogCancel disabled={isImportingFeatured} onClick={dismissImportDialog}>
              {t("onboarding.importFeatured.cancel")}
            </AlertDialogCancel>
            <button
              onClick={confirmImportFeatured}
              disabled={isImportingFeatured}
              className="apple-button-primary h-10 px-4 flex items-center gap-2 disabled:opacity-50"
            >
              {isImportingFeatured ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  {t("onboarding.importFeatured.importing")}
                </>
              ) : (
                t("onboarding.importFeatured.confirm")
              )}
            </button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Title Bar - Apple 风格：极简、透明感 */}
      <header
        data-tauri-drag-region
        className="h-12 flex-shrink-0 flex items-center justify-between px-4 bg-sidebar/80 backdrop-blur-xl border-b border-border/50"
      >
        {/* macOS: 左侧窗口控件 */}
        {platform === "macos" && (
          <div className="w-[70px]">
            <WindowControls />
          </div>
        )}

        {/* Windows: 左侧应用图标 + 标题 */}
        {platform === "windows" && (
          <div className="flex items-center gap-2 select-none">
            <img src={appIconUrl} alt="" className="w-5 h-5" draggable={false} />
            <div className="text-[13px] font-medium text-foreground/80">Agent Skills Guard</div>
          </div>
        )}

        {/* 中间占位 */}
        <div className="flex-1" />

        {/* 右侧：更新徽章 */}
        <div className="flex items-center gap-3">
          <UpdateBadge />
          {/* Windows/Linux: 右侧窗口控件 */}
          {platform !== "macos" && platform !== null && <WindowControls />}
        </div>
      </header>

      {/* Main Area: Sidebar + Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Sidebar */}
        <Sidebar currentTab={currentTab} onTabChange={setCurrentTab} />

        {/* Content Area - 更大的内边距，更宽敞的感觉 */}
        <main className="flex-1 overflow-hidden">
          <Suspense fallback={<PageFallback />}>
            {currentTab === "overview" && (
              <div className="h-full overflow-y-auto">
                <div className="p-8 animate-fade-in">
                  <div className="max-w-6xl mx-auto">
                    <OverviewPage />
                  </div>
                </div>
              </div>
            )}
            {currentTab === "installed" && (
              <div className="h-full overflow-hidden">
                <InstalledSkillsPage />
              </div>
            )}
            {currentTab === "marketplace" && (
              <div className="h-full overflow-hidden">
                <MarketplacePage
                  onNavigateToRepositories={() => setCurrentTab("repositories")}
                  onNavigateToOverview={() => setCurrentTab("overview")}
                  presetFilter={marketplacePreset ?? undefined}
                  onPresetApplied={clearMarketplacePreset}
                />
              </div>
            )}
            {currentTab === "repositories" && (
              <div className="h-full overflow-y-auto">
                <div className="p-8 animate-fade-in">
                  <div className="max-w-6xl mx-auto">
                    <RepositoriesPage
                      onNavigateToMarket={(options) => {
                        setMarketplacePreset(
                          options?.marketplaceName
                            ? { marketplaceName: options.marketplaceName }
                            : null
                        );
                        setCurrentTab("marketplace");
                      }}
                    />
                  </div>
                </div>
              </div>
            )}
            {currentTab === "settings" && (
              <div className="h-full overflow-y-auto">
                <div className="p-8 animate-fade-in">
                  <div className="max-w-6xl mx-auto">
                    <SettingsPage />
                  </div>
                </div>
              </div>
            )}
          </Suspense>
        </main>
      </div>
    </div>
  );
}

function App() {
  return (
    <QueryClientProvider client={reactQueryClient}>
      <AppContent />
      <Toaster
        position="top-right"
        expand
        gap={12}
        offset={{ top: 64, right: 16 }}
        mobileOffset={{ top: 12, right: 12 }}
        toastOptions={{
          duration: 3000,
          style: { fontFamily: "inherit", fontSize: "14px" },
          classNames: {
            toast:
              "!rounded-2xl !border !border-border/70 !bg-card/70 !shadow-[0_6px_16px_rgba(0,0,0,0.12)] !backdrop-blur-md",
            default: "!text-foreground",
            success: "!text-success",
            info: "!text-primary",
            warning: "!text-warning",
            error: "!text-destructive",
          },
        }}
      />
    </QueryClientProvider>
  );
}

export default App;
