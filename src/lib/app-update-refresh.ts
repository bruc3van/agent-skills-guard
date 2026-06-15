import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import { api } from "./api";

export const APP_VERSION_SKILL_REFRESH_KEY = "agent-skills-guard:skill-state-refreshed-version";

const SKILL_STATE_QUERY_KEYS: QueryKey[] = [
  ["skills"],
  ["skills", "installed"],
  ["scanResults"],
  AGENT_TOOLS_KEY,
];

type AppVersionSkillRefreshDeps = {
  storage?: Pick<Storage, "getItem" | "setItem">;
  refreshSkillLinks?: () => Promise<Awaited<ReturnType<typeof api.refreshSkillLinks>>>;
  scanLocalSkills?: () => Promise<unknown>;
  getInstalledSkills?: () => Promise<Awaited<ReturnType<typeof api.getInstalledSkills>>>;
};

function getLocalStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

async function refetchSkillStateQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all(
    SKILL_STATE_QUERY_KEYS.map((queryKey) =>
      queryClient.refetchQueries({ queryKey, exact: true, type: "all" })
    )
  );
}

/**
 * 扫描磁盘上的本地技能/软链接，写入数据库后拉取已安装列表并刷新缓存。
 * 顺序很重要：先强制按磁盘状态重建 linked_tools（含 Claude Code / Codex 软链检测），
 * 再 getInstalledSkills 写入缓存，避免陈旧 queryFn 结果覆盖刷新后的 linked_tools。
 */
export async function refreshSkillStateFromDisk(
  queryClient: QueryClient,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<void> {
  // refresh_skill_links 会重新检测各工具 skill 目录下的软链并写回 DB，
  // 比 scan_local_skills 更直接：scan 只在遍历到链接目录时关联工具，
  // 而 refresh 会针对每条已安装记录补齐 linked_tools。
  if (deps.refreshSkillLinks) {
    await deps.refreshSkillLinks();
  } else {
    await api.refreshSkillLinks();
  }
  // scan 作为兜底：发现 DB 中尚未记录的本地技能（未通过本工具安装的）。
  await (deps.scanLocalSkills ?? api.scanLocalSkills)();

  await refetchSkillStateQueries(queryClient);

  // 在 refetch 之后写入，避免陈旧 queryFn 结果覆盖刷新后的 linked_tools
  const installed = await (deps.getInstalledSkills ?? api.getInstalledSkills)();
  queryClient.setQueryData(["skills", "installed"], installed);
}

/** 应用内更新安装完成后调用（Windows 未重启前也会尝试同步链接状态）。 */
export async function refetchSkillStateAfterAppUpdate(
  queryClient: QueryClient,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<void> {
  await refreshSkillStateFromDisk(queryClient, deps);
}

/**
 * 应用冷启动时同步技能与工具链接状态。
 * App.tsx 通过 ref 保证每个进程只调用一次；此处不再按版本号跳过扫描。
 */
export async function reconcileSkillStateOnAppStartup(
  queryClient: QueryClient,
  currentVersion: string,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<boolean> {
  await refreshSkillStateFromDisk(queryClient, deps);

  const storage = deps.storage ?? getLocalStorage();
  if (storage) {
    storage.setItem(APP_VERSION_SKILL_REFRESH_KEY, currentVersion);
  }

  return true;
}

/** @deprecated 请使用 reconcileSkillStateOnAppStartup */
export async function reconcileSkillStateAfterAppVersionChange(
  queryClient: QueryClient,
  currentVersion: string,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<boolean> {
  return reconcileSkillStateOnAppStartup(queryClient, currentVersion, deps);
}
