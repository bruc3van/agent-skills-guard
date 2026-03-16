import type { SecurityIssue, SecurityReport } from "./security";

export type TabType = "overview" | "marketplace" | "installed" | "repositories" | "settings";

export interface Repository {
  id: string;
  url: string;
  name: string;
  description?: string;
  enabled: boolean;
  scan_subdirs: boolean;
  added_at: string;
  last_scanned?: string;
  // 新增:缓存字段
  cache_path?: string;
  cached_at?: string;
  cached_commit_sha?: string;
}

export interface ImportFeaturedRepositoriesResult {
  total_count: number;
  added_count: number;
  skipped_count: number;
}

export interface Skill {
  id: string;
  name: string;
  description?: string;
  repository_url: string;
  repository_owner?: string; // 仓库所有者,如 "anthropics" 或 "local"
  file_path: string;
  version?: string;
  author?: string;
  installed: boolean;
  installed_at?: string;
  local_path?: string; // 向后兼容,保留单个路径字段
  local_paths?: string[]; // 新增:支持多个安装路径
  checksum?: string;
  security_score?: number;
  security_issues?: SecurityIssue[];
  security_report?: SecurityReport;
  installed_commit_sha?: string; // 安装时的 commit SHA，用于版本追踪
}

export interface Plugin {
  id: string;
  claude_id?: string;
  name: string;
  description?: string;
  version?: string;
  installed_version?: string;
  author?: string;
  repository_url: string;
  repository_owner?: string;
  marketplace_name: string;
  source: string;
  discovery_source?: string;
  marketplace_add_command?: string;
  plugin_install_command?: string;
  installed: boolean;
  installed_at?: string;
  claude_scope?: string;
  claude_enabled?: boolean;
  claude_install_path?: string;
  claude_last_updated?: string;
  security_score?: number;
  security_issues?: SecurityIssue[];
  security_level?: string;
  security_report?: SecurityReport;
  scanned_at?: string;
  staging_path?: string;
  install_log?: string;
  install_status?: string;
}

export interface PluginInstallStatus {
  plugin_id: string;
  plugin_name: string;
  status: string;
  output: string;
}

export interface PluginInstallResult {
  marketplace_name: string;
  marketplace_repo: string;
  marketplace_status: string;
  raw_log: string;
  plugin_statuses: PluginInstallStatus[];
}

export interface PluginUninstallResult {
  plugin_id: string;
  plugin_name: string;
  success: boolean;
  raw_log: string;
}

export interface MarketplaceRemoveResult {
  marketplace_name: string;
  marketplace_repo: string;
  success: boolean;
  removed_plugins_count: number;
  raw_log: string;
}

export interface ClaudeMarketplace {
  name: string;
  source?: string;
  repo?: string;
  repository_url?: string;
  install_location?: string;
}

export interface PluginUpdateResult {
  plugin_id: string;
  plugin_name: string;
  status: string;
  raw_log: string;
}

export interface MarketplaceUpdateResult {
  marketplace_name: string;
  success: boolean;
  raw_log: string;
}

export interface SkillPluginUpgradeCandidate {
  skill_id: string;
  skill_name: string;
  plugin_id: string; // name@marketplace
  plugin_name: string;
  marketplace_name: string;
  marketplace_repo?: string;
  marketplace_repository_url?: string;
  marketplace_add_command?: string;
  latest_version?: string;
  reason: string;
}

export enum SecurityLevel {
  Safe = "Safe",
  Low = "Low",
  Medium = "Medium",
  High = "High",
  Critical = "Critical",
}

export type { CacheStats, ClearAllCachesResult } from "./cache";
export type {
  FeaturedRepositoriesConfig,
  FeaturedRepository,
  FeaturedRepositoryCategory,
} from "./featured";
export type {
  FeaturedMarketplacesConfig,
  FeaturedMarketplace,
  FeaturedMarketplaceCategory,
  FeaturedMarketplacePlugin,
  FeaturedMarketplaceOwner,
  LocalizedText,
} from "./featured-marketplace";

export interface InstallPathSelection {
  type: "user" | "recent" | "custom";
  path: string;
  displayName: string;
}
