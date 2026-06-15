import { invoke } from "@tauri-apps/api/core";
import { safeInvoke } from "./api-error";
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
  AgentToolInfo,
  LocalCliTool,
} from "../types";
import type { SecurityReport } from "../types/security";
import type { SkillScanResult } from "../types/security";

export const api = {
  // Repository APIs
  async addRepository(url: string, name: string): Promise<string> {
    return safeInvoke(invoke<string>("add_repository", { url, name }));
  },

  async getRepositories(): Promise<Repository[]> {
    return safeInvoke(invoke<Repository[]>("get_repositories"));
  },

  async deleteRepository(repoId: string): Promise<void> {
    return safeInvoke(invoke<void>("delete_repository", { repoId }));
  },

  async scanRepository(repoId: string): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("scan_repository", { repoId }));
  },

  // Skill APIs
  async getSkills(): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("get_skills"));
  },

  async getInstalledSkills(): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("get_installed_skills"));
  },

  async installSkill(
    skillId: string,
    installPath?: string,
    allowPartialScan = false
  ): Promise<void> {
    return safeInvoke(
      invoke<void>("install_skill", {
        skillId,
        installPath: installPath || null,
        allowPartialScan,
      })
    );
  },

  async prepareSkillInstallation(
    skillId: string,
    locale: string,
    allowPartialScan = false
  ): Promise<SecurityReport> {
    return safeInvoke(
      invoke<SecurityReport>("prepare_skill_installation", {
        skillId,
        locale,
        allowPartialScan,
      })
    );
  },

  async confirmSkillInstallation(
    skillId: string,
    installPath?: string,
    allowPartialScan = false,
    targetTools?: string[]
  ): Promise<void> {
    return safeInvoke(
      invoke<void>("confirm_skill_installation", {
        skillId,
        installPath: installPath || null,
        allowPartialScan,
        targetTools: targetTools ?? null,
      })
    );
  },

  async cancelSkillInstallation(skillId: string): Promise<void> {
    return safeInvoke(invoke<void>("cancel_skill_installation", { skillId }));
  },

  async getDefaultInstallPath(): Promise<string> {
    return safeInvoke(invoke<string>("get_default_install_path"));
  },

  async selectCustomInstallPath(): Promise<string | null> {
    return safeInvoke(invoke<string | null>("select_custom_install_path"));
  },

  async getScanResults(): Promise<SkillScanResult[]> {
    return safeInvoke(invoke<SkillScanResult[]>("get_scan_results"));
  },

  async scanAllInstalledSkills(
    locale: string,
    scanParallelism?: number
  ): Promise<SkillScanResult[]> {
    return safeInvoke(
      invoke<SkillScanResult[]>("scan_all_installed_skills", {
        locale,
        scanParallelism: scanParallelism ?? null,
      })
    );
  },

  async uninstallSkill(skillId: string): Promise<void> {
    return safeInvoke(invoke<void>("uninstall_skill", { skillId }));
  },

  async uninstallSkillPath(skillId: string, path: string): Promise<void> {
    return safeInvoke(invoke<void>("uninstall_skill_path", { skillId, path }));
  },

  async deleteSkill(skillId: string): Promise<void> {
    return safeInvoke(invoke<void>("delete_skill", { skillId }));
  },

  // Scan local skills directory
  async scanLocalSkills(): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("scan_local_skills"));
  },

  // 强制按磁盘实际状态刷新已安装技能的工具链接（软链检测）
  async refreshSkillLinks(): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("refresh_skill_links"));
  },

  // 缓存管理
  async clearRepositoryCache(repoId: string): Promise<void> {
    return safeInvoke(invoke<void>("clear_repository_cache", { repoId }));
  },

  async clearAllRepositoryCaches(): Promise<ClearAllCachesResult> {
    return safeInvoke(invoke<ClearAllCachesResult>("clear_all_repository_caches"));
  },

  async refreshRepositoryCache(repoId: string): Promise<Skill[]> {
    return safeInvoke(invoke<Skill[]>("refresh_repository_cache", { repoId }));
  },

  async getCacheStats(): Promise<CacheStats> {
    return safeInvoke(invoke<CacheStats>("get_cache_stats"));
  },

  // 打开技能目录
  async openSkillDirectory(localPath: string): Promise<void> {
    return safeInvoke(invoke<void>("open_skill_directory", { localPath }));
  },

  // Featured repositories
  async getFeaturedRepositories(): Promise<FeaturedRepositoriesConfig> {
    return safeInvoke(invoke<FeaturedRepositoriesConfig>("get_featured_repositories"));
  },

  async refreshFeaturedRepositories(): Promise<FeaturedRepositoriesConfig> {
    return safeInvoke(invoke<FeaturedRepositoriesConfig>("refresh_featured_repositories"));
  },

  async getFeaturedMarketplaces(): Promise<FeaturedMarketplacesConfig> {
    return safeInvoke(invoke<FeaturedMarketplacesConfig>("get_featured_marketplaces"));
  },

  async refreshFeaturedMarketplaces(): Promise<FeaturedMarketplacesConfig> {
    return safeInvoke(invoke<FeaturedMarketplacesConfig>("refresh_featured_marketplaces"));
  },

  async importFeaturedRepositories(
    categoryIds?: string[]
  ): Promise<ImportFeaturedRepositoriesResult> {
    return safeInvoke(
      invoke<ImportFeaturedRepositoriesResult>("import_featured_repositories", {
        categoryIds: categoryIds || null,
      })
    );
  },

  async isRepositoryAdded(url: string): Promise<boolean> {
    return safeInvoke(invoke<boolean>("is_repository_added", { url }));
  },

  // Skill Update APIs
  async checkSkillsUpdates(): Promise<Array<[string, string]>> {
    return safeInvoke(invoke<Array<[string, string]>>("check_skills_updates"));
  },

  async prepareSkillUpdate(skillId: string, locale: string): Promise<[SecurityReport, string[]]> {
    return safeInvoke(
      invoke<[SecurityReport, string[]]>("prepare_skill_update", { skillId, locale })
    );
  },

  async confirmSkillUpdate(
    skillId: string,
    forceOverwrite: boolean,
    allowPartialScan = false
  ): Promise<void> {
    return safeInvoke(
      invoke<void>("confirm_skill_update", { skillId, forceOverwrite, allowPartialScan })
    );
  },

  async cancelSkillUpdate(skillId: string): Promise<void> {
    return safeInvoke(invoke<void>("cancel_skill_update", { skillId }));
  },

  // 自动扫描未扫描的仓库（首次启动）
  async autoScanUnscannedRepositories(): Promise<string[]> {
    return safeInvoke(invoke<string[]>("auto_scan_unscanned_repositories"));
  },

  // Plugin APIs
  async getPlugins(locale?: string): Promise<Plugin[]> {
    return safeInvoke(invoke<Plugin[]>("get_plugins", { locale: locale || null }));
  },

  async syncFeaturedMarketplacePlugins(locale: string): Promise<Plugin[]> {
    return safeInvoke(invoke<Plugin[]>("sync_featured_marketplace_plugins", { locale }));
  },

  async preparePluginInstallation(pluginId: string, locale: string): Promise<SecurityReport> {
    return safeInvoke(invoke<SecurityReport>("prepare_plugin_installation", { pluginId, locale }));
  },

  async confirmPluginInstallation(
    pluginId: string,
    claudeCommand?: string
  ): Promise<PluginInstallResult> {
    return safeInvoke(
      invoke<PluginInstallResult>("confirm_plugin_installation", {
        pluginId,
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async cancelPluginInstallation(pluginId: string): Promise<void> {
    return safeInvoke(invoke<void>("cancel_plugin_installation", { pluginId }));
  },

  async uninstallPlugin(pluginId: string, claudeCommand?: string): Promise<PluginUninstallResult> {
    return safeInvoke(
      invoke<PluginUninstallResult>("uninstall_plugin", {
        pluginId,
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async removeMarketplace(
    marketplaceName: string,
    marketplaceRepo: string,
    claudeCommand?: string
  ): Promise<MarketplaceRemoveResult> {
    return safeInvoke(
      invoke<MarketplaceRemoveResult>("remove_marketplace", {
        marketplaceName,
        marketplaceRepo,
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async getClaudeMarketplaces(claudeCommand?: string): Promise<ClaudeMarketplace[]> {
    return safeInvoke(
      invoke<ClaudeMarketplace[]>("get_claude_marketplaces", {
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async getPluginsCached(): Promise<Plugin[]> {
    return safeInvoke(invoke<Plugin[]>("get_plugins_cached"));
  },

  async checkPluginsUpdates(claudeCommand?: string): Promise<Array<[string, string]>> {
    return safeInvoke(
      invoke<Array<[string, string]>>("check_plugins_updates", {
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async updatePlugin(pluginId: string, claudeCommand?: string): Promise<PluginUpdateResult> {
    return safeInvoke(
      invoke<PluginUpdateResult>("update_plugin", {
        pluginId,
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async checkMarketplacesUpdates(claudeCommand?: string): Promise<Array<[string, string]>> {
    return safeInvoke(
      invoke<Array<[string, string]>>("check_marketplaces_updates", {
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async updateMarketplace(
    marketplaceName: string,
    claudeCommand?: string
  ): Promise<MarketplaceUpdateResult> {
    return safeInvoke(
      invoke<MarketplaceUpdateResult>("update_marketplace", {
        marketplaceName,
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async getSkillPluginUpgradeCandidates(
    claudeCommand?: string
  ): Promise<SkillPluginUpgradeCandidate[]> {
    return safeInvoke(
      invoke<SkillPluginUpgradeCandidate[]>("get_skill_plugin_upgrade_candidates", {
        claudeCommand: claudeCommand || null,
      })
    );
  },

  async scanAllInstalledPlugins(
    locale: string,
    claudeCommand?: string,
    scanParallelism?: number
  ): Promise<string[]> {
    return safeInvoke(
      invoke<string[]>("scan_all_installed_plugins", {
        locale,
        claudeCommand: claudeCommand || null,
        scanParallelism: scanParallelism ?? null,
      })
    );
  },

  async scanInstalledSkill(
    skillId: string,
    locale: string,
    scanId?: string
  ): Promise<SkillScanResult> {
    return safeInvoke(
      invoke<SkillScanResult>("scan_installed_skill", { skillId, locale, scanId: scanId || null })
    );
  },

  async scanInstalledPlugin(
    pluginId: string,
    locale: string,
    claudeCommand?: string,
    scanId?: string,
    skipSync?: boolean
  ): Promise<string> {
    return safeInvoke(
      invoke<string>("scan_installed_plugin", {
        pluginId,
        locale,
        claudeCommand: claudeCommand || null,
        scanId: scanId || null,
        skipSync: skipSync ?? null,
      })
    );
  },

  async countScanFiles(dirPath: string, skipReadme = true): Promise<number> {
    return safeInvoke(invoke<number>("count_scan_files", { dirPath, skipReadme }));
  },

  // Reset
  async resetAppData(): Promise<void> {
    return safeInvoke(invoke<void>("reset_app_data"));
  },

  // Agent Tools
  async listAgentTools(): Promise<AgentToolInfo[]> {
    return safeInvoke(invoke<AgentToolInfo[]>("list_agent_tools"));
  },

  async syncSkillToTools(skillId: string, tools: string[]): Promise<void> {
    return safeInvoke(invoke<void>("sync_skill_to_tools", { skillId, tools }));
  },

  async syncAllSkillsToTools(tools: string[]): Promise<void> {
    return safeInvoke(invoke<void>("sync_all_skills_to_tools", { tools }));
  },

  // 本地 CLI 管理
  async listLocalCliTools(): Promise<LocalCliTool[]> {
    return safeInvoke(invoke<LocalCliTool[]>("list_local_cli_tools"));
  },

  async checkLocalCliUpdates(): Promise<LocalCliTool[]> {
    return safeInvoke(invoke<LocalCliTool[]>("check_local_cli_updates"));
  },

  async rescanLocalCliTools(): Promise<LocalCliTool[]> {
    return safeInvoke(invoke<LocalCliTool[]>("rescan_local_cli_tools"));
  },

  async updateLocalCliTool(toolPath: string): Promise<string> {
    return safeInvoke(invoke<string>("update_local_cli_tool", { toolPath }));
  },

  async uninstallLocalCliTool(toolPath: string): Promise<string> {
    return safeInvoke(invoke<string>("uninstall_local_cli_tool", { toolPath }));
  },

  async openLocalCliFolder(toolPath: string): Promise<void> {
    return safeInvoke(invoke<void>("open_local_cli_folder", { toolPath }));
  },

  async fetchLocalCliDescriptions(toolPaths: string[]): Promise<Array<[string, string]>> {
    return safeInvoke(
      invoke<Array<[string, string]>>("fetch_local_cli_descriptions", { toolPaths })
    );
  },
};
