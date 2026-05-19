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

export async function refetchSkillStateAfterAppUpdate(queryClient: QueryClient): Promise<void> {
  await refetchSkillStateQueries(queryClient);
}

async function refetchSkillStateQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all(
    SKILL_STATE_QUERY_KEYS.map((queryKey) =>
      queryClient.refetchQueries({ queryKey, exact: true, type: "all" })
    )
  );
}

type AppVersionSkillRefreshDeps = {
  storage?: Pick<Storage, "getItem" | "setItem">;
  scanLocalSkills?: () => Promise<unknown>;
};

function getLocalStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export async function reconcileSkillStateAfterAppVersionChange(
  queryClient: QueryClient,
  currentVersion: string,
  deps: AppVersionSkillRefreshDeps = {}
): Promise<boolean> {
  const storage = deps.storage ?? getLocalStorage();
  if (!storage) return false;

  if (storage.getItem(APP_VERSION_SKILL_REFRESH_KEY) === currentVersion) {
    return false;
  }

  await (deps.scanLocalSkills ?? api.scanLocalSkills)();
  await refetchSkillStateQueries(queryClient);
  storage.setItem(APP_VERSION_SKILL_REFRESH_KEY, currentVersion);

  return true;
}
