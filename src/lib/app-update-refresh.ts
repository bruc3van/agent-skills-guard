import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import { api } from "./api";

export const APP_VERSION_SKILL_REFRESH_KEY = "agent-skills-guard:skill-state-refreshed-version";
export const APP_SESSION_SKILL_REFRESH_KEY = "agent-skills-guard:skill-state-refreshed-session";

const SKILL_STATE_QUERY_KEYS: QueryKey[] = [
  ["skills"],
  ["skills", "installed"],
  ["scanResults"],
  AGENT_TOOLS_KEY,
];

type AppVersionSkillRefreshDeps = {
  storage?: Pick<Storage, "getItem" | "setItem">;
  sessionStorage?: Pick<Storage, "getItem" | "setItem">;
  scanLocalSkills?: () => Promise<unknown>;
};

function getLocalStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function getSessionStorage(): Storage | null {
  try {
    return globalThis.sessionStorage ?? null;
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

/** 扫描磁盘上的本地技能/软链接，并刷新相关 React Query 缓存。 */
export async function refreshSkillStateFromDisk(
  queryClient: QueryClient,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<void> {
  await (deps.scanLocalSkills ?? api.scanLocalSkills)();
  await refetchSkillStateQueries(queryClient);
}

/** 应用内更新安装完成后调用（含 GitHub 安装包覆盖同版本场景）。 */
export async function refetchSkillStateAfterAppUpdate(
  queryClient: QueryClient,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<void> {
  const sessionStorage = deps.sessionStorage ?? getSessionStorage();
  await refreshSkillStateFromDisk(queryClient, deps);
  if (sessionStorage) {
    sessionStorage.setItem(APP_SESSION_SKILL_REFRESH_KEY, "1");
  }
}

/**
 * 应用冷启动时同步技能与工具链接状态。
 * 每个会话至少执行一次完整扫描，版本变更时也会更新持久化版本标记。
 */
export async function reconcileSkillStateOnAppStartup(
  queryClient: QueryClient,
  currentVersion: string,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<boolean> {
  const storage = deps.storage ?? getLocalStorage();
  const sessionStorage = deps.sessionStorage ?? getSessionStorage();
  if (!sessionStorage) return false;

  const versionChanged = storage?.getItem(APP_VERSION_SKILL_REFRESH_KEY) !== currentVersion;
  const sessionAlreadyRefreshed =
    sessionStorage.getItem(APP_SESSION_SKILL_REFRESH_KEY) === "1";

  if (!versionChanged && sessionAlreadyRefreshed) {
    return false;
  }

  await refreshSkillStateFromDisk(queryClient, deps);

  if (storage) {
    storage.setItem(APP_VERSION_SKILL_REFRESH_KEY, currentVersion);
  }
  sessionStorage.setItem(APP_SESSION_SKILL_REFRESH_KEY, "1");

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