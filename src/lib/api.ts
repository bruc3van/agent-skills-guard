import { invoke } from "@tauri-apps/api/core";
import type {
  Repository,
  ImportFeaturedRepositoriesResult,
  Skill,
  Plugin,
  ClaudeMarketplace,
  PluginInstallResult,
  PluginUninstallResult,
  MarketplaceRemoveResult,
  PluginUpdateResult,
  MarketplaceUpdateResult,
  SkillPluginUpgradeCandidate,
  CacheStats,
  FeaturedRepositoriesConfig,
  FeaturedMarketplacesConfig,
  ClearAllCachesResult,
} from "../types";
import type { SecurityReport } from "../types/security";
import type { SkillScanResult } from "../types/security";

export const api = {
  // Repository APIs
  async addRepository(url: string, name: string): Promise<string> {
    return invoke("add_repository", { url, name });
  },

  async getRepositories(): Promise<Repository[]> {
    return invoke("get_repositories");
  },

  async deleteRepository(repoId: string): Promise<void> {
    return invoke("delete_repository", { repoId });
  },

  async scanRepository(repoId: string): Promise<Skill[]> {
    return invoke("scan_repository", { repoId });
  },

  // Skill APIs
  async getSkills(): Promise<Skill[]> {
    return invoke("get_skills");
  },

  async getInstalledSkills(): Promise<Skill[]> {
    return invoke("get_installed_skills");
  },

  async installSkill(
    skillId: string,
    installPath?: string,
    allowPartialScan = false
  ): Promise<void> {
    return invoke("install_skill", {
      skillId,
      installPath: installPath || null,
      allowPartialScan,
    });
  },

  async confirmSkillInstallation(
    skillId: string,
    installPath?: string,
    allowPartialScan = false
  ): Promise<void> {
    return invoke("confirm_skill_installation", {
      skillId,
      installPath: installPath || null,
      allowPartialScan,
    });
  },

  async uninstallSkill(skillId: string): Promise<void> {
    return invoke("uninstall_skill", { skillId });
  },

  async uninstallSkillPath(skillId: string, path: string): Promise<void> {
    return invoke("uninstall_skill_path", { skillId, path });
  },

  async deleteSkill(skillId: string): Promise<void> {
    return invoke("delete_skill", { skillId });
  },

  // Scan local skills directory
  async scanLocalSkills(): Promise<Skill[]> {
    return invoke("scan_local_skills");
  },

  // 缓存管理
  async clearRepositoryCache(repoId: string): Promise<void> {
    return invoke("clear_repository_cache", { repoId });
  },

  async clearAllRepositoryCaches(): Promise<ClearAllCachesResult> {
    return invoke("clear_all_repository_caches");
  },

  async refreshRepositoryCache(repoId: string): Promise<Skill[]> {
    return invoke("refresh_repository_cache", { repoId });
  },

  async getCacheStats(): Promise<CacheStats> {
    return invoke("get_cache_stats");
  },

  // 打开技能目录
  async openSkillDirectory(localPath: string): Promise<void> {
    return invoke("open_skill_directory", { localPath });
  },

  // Featured repositories
  async getFeaturedRepositories(): Promise<FeaturedRepositoriesConfig> {
    return invoke("get_featured_repositories");
  },

  async refreshFeaturedRepositories(): Promise<FeaturedRepositoriesConfig> {
    return invoke("refresh_featured_repositories");
  },

  async getFeaturedMarketplaces(): Promise<FeaturedMarketplacesConfig> {
    return invoke("get_featured_marketplaces");
  },

  async refreshFeaturedMarketplaces(): Promise<FeaturedMarketplacesConfig> {
    return invoke("refresh_featured_marketplaces");
  },

  async importFeaturedRepositories(categoryIds?: string[]): Promise<ImportFeaturedRepositoriesResult> {
    return invoke("import_featured_repositories", { categoryIds: categoryIds || null });
  },

  async isRepositoryAdded(url: string): Promise<boolean> {
    return invoke("is_repository_added", { url });
  },

  // Skill Update APIs
  async checkSkillsUpdates(): Promise<Array<[string, string]>> {
    return invoke("check_skills_updates");
  },

  async prepareSkillUpdate(skillId: string, locale: string): Promise<[SecurityReport, string[]]> {
    return invoke("prepare_skill_update", { skillId, locale });
  },

  async confirmSkillUpdate(
    skillId: string,
    forceOverwrite: boolean,
    allowPartialScan = false
  ): Promise<void> {
    return invoke("confirm_skill_update", { skillId, forceOverwrite, allowPartialScan });
  },

  async cancelSkillUpdate(skillId: string): Promise<void> {
    return invoke("cancel_skill_update", { skillId });
  },

  // 自动扫描未扫描的仓库（首次启动）
  async autoScanUnscannedRepositories(): Promise<string[]> {
    return invoke("auto_scan_unscanned_repositories");
  },

  // Plugin APIs
  async getPlugins(locale?: string): Promise<Plugin[]> {
    return invoke("get_plugins", { locale: locale || null });
  },

  async syncFeaturedMarketplacePlugins(locale: string): Promise<Plugin[]> {
    return invoke("sync_featured_marketplace_plugins", { locale });
  },

  async preparePluginInstallation(pluginId: string, locale: string): Promise<SecurityReport> {
    return invoke("prepare_plugin_installation", { pluginId, locale });
  },

  async confirmPluginInstallation(
    pluginId: string,
    claudeCommand?: string
  ): Promise<PluginInstallResult> {
    return invoke("confirm_plugin_installation", {
      pluginId,
      claudeCommand: claudeCommand || null,
    });
  },

  async cancelPluginInstallation(pluginId: string): Promise<void> {
    return invoke("cancel_plugin_installation", { pluginId });
  },

  async uninstallPlugin(pluginId: string, claudeCommand?: string): Promise<PluginUninstallResult> {
    return invoke("uninstall_plugin", {
      pluginId,
      claudeCommand: claudeCommand || null,
    });
  },

  async removeMarketplace(
    marketplaceName: string,
    marketplaceRepo: string,
    claudeCommand?: string
  ): Promise<MarketplaceRemoveResult> {
    return invoke("remove_marketplace", {
      marketplaceName,
      marketplaceRepo,
      claudeCommand: claudeCommand || null,
    });
  },

  async getClaudeMarketplaces(claudeCommand?: string): Promise<ClaudeMarketplace[]> {
    return invoke("get_claude_marketplaces", { claudeCommand: claudeCommand || null });
  },

  async getPluginsCached(): Promise<Plugin[]> {
    return invoke("get_plugins_cached");
  },

  async checkPluginsUpdates(claudeCommand?: string): Promise<Array<[string, string]>> {
    return invoke("check_plugins_updates", { claudeCommand: claudeCommand || null });
  },

  async updatePlugin(pluginId: string, claudeCommand?: string): Promise<PluginUpdateResult> {
    return invoke("update_plugin", { pluginId, claudeCommand: claudeCommand || null });
  },

  async checkMarketplacesUpdates(claudeCommand?: string): Promise<Array<[string, string]>> {
    return invoke("check_marketplaces_updates", { claudeCommand: claudeCommand || null });
  },

  async updateMarketplace(
    marketplaceName: string,
    claudeCommand?: string
  ): Promise<MarketplaceUpdateResult> {
    return invoke("update_marketplace", {
      marketplaceName,
      claudeCommand: claudeCommand || null,
    });
  },

  async getSkillPluginUpgradeCandidates(
    claudeCommand?: string
  ): Promise<SkillPluginUpgradeCandidate[]> {
    return invoke("get_skill_plugin_upgrade_candidates", { claudeCommand: claudeCommand || null });
  },

  async scanAllInstalledPlugins(
    locale: string,
    claudeCommand?: string,
    scanParallelism?: number
  ): Promise<string[]> {
    return invoke("scan_all_installed_plugins", {
      locale,
      claudeCommand: claudeCommand || null,
      scanParallelism: scanParallelism ?? null,
    });
  },

  async scanInstalledSkill(skillId: string, locale: string, scanId?: string): Promise<SkillScanResult> {
    return invoke("scan_installed_skill", { skillId, locale, scanId: scanId || null });
  },

  async scanInstalledPlugin(
    pluginId: string,
    locale: string,
    claudeCommand?: string,
    scanId?: string,
    skipSync?: boolean
  ): Promise<string> {
    return invoke("scan_installed_plugin", {
      pluginId,
      locale,
      claudeCommand: claudeCommand || null,
      scanId: scanId || null,
      skipSync: skipSync ?? null,
    });
  },

  async countScanFiles(dirPath: string, skipReadme = true): Promise<number> {
    return invoke("count_scan_files", { dirPath, skipReadme });
  },

  // Reset
  async resetAppData(): Promise<void> {
    return invoke("reset_app_data");
  },
};
