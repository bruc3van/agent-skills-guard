import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import {
  APP_VERSION_SKILL_REFRESH_KEY,
  reconcileSkillStateOnAppStartup,
  refetchSkillStateAfterAppUpdate,
} from "./app-update-refresh";

function createMemoryStorage(initial: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(initial));

  return {
    get length() {
      return values.size;
    },
    clear() {
      values.clear();
    },
    getItem(key: string) {
      return values.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(values.keys())[index] ?? null;
    },
    removeItem(key: string) {
      values.delete(key);
    },
    setItem(key: string, value: string) {
      values.set(key, value);
    },
  };
}

async function prefetchSkillQueries(queryClient: QueryClient, calls: string[]) {
  await queryClient.prefetchQuery({
    queryKey: ["skills"],
    queryFn: async () => {
      calls.push("skills");
      return [];
    },
  });
  await queryClient.prefetchQuery({
    queryKey: ["skills", "installed"],
    queryFn: async () => {
      calls.push("installed");
      return [{ id: "stale", linked_tools: [] }];
    },
  });
  await queryClient.prefetchQuery({
    queryKey: ["scanResults"],
    queryFn: async () => {
      calls.push("scanResults");
      return [];
    },
  });
  await queryClient.prefetchQuery({
    queryKey: AGENT_TOOLS_KEY,
    queryFn: async () => {
      calls.push("agentTools");
      return [];
    },
  });
}

describe("refetchSkillStateAfterAppUpdate", () => {
  it("rescans, hydrates installed cache, then refetches related queries", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: Infinity,
        },
      },
    });
    const calls: string[] = [];
    await prefetchSkillQueries(queryClient, calls);

    let scanCount = 0;
    calls.length = 0;

    await refetchSkillStateAfterAppUpdate(queryClient, {
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
      getInstalledSkills: async () => {
        calls.push("getInstalled");
        return [{ id: "fresh", linked_tools: ["claude-code"] }];
      },
    });

    expect(scanCount).toBe(1);
    expect(queryClient.getQueryData(["skills", "installed"])).toEqual([
      { id: "fresh", linked_tools: ["claude-code"] },
    ]);
    expect(calls.sort()).toEqual(["agentTools", "getInstalled", "installed", "scanResults", "skills"]);
  });
});

describe("reconcileSkillStateOnAppStartup", () => {
  it("always rescans on startup even when the stored version is unchanged", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: Infinity,
        },
      },
    });
    const calls: string[] = [];
    await prefetchSkillQueries(queryClient, calls);

    const storage = createMemoryStorage({
      [APP_VERSION_SKILL_REFRESH_KEY]: "1.2.5",
    });
    let scanCount = 0;
    calls.length = 0;

    const didRefresh = await reconcileSkillStateOnAppStartup(queryClient, "1.2.5", {
      storage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
      getInstalledSkills: async () => {
        calls.push("getInstalled");
        return [{ id: "fresh", linked_tools: ["claude-code"] }];
      },
    });

    expect(didRefresh).toBe(true);
    expect(scanCount).toBe(1);
    expect(storage.getItem(APP_VERSION_SKILL_REFRESH_KEY)).toBe("1.2.5");
    expect(queryClient.getQueryData(["skills", "installed"])).toEqual([
      { id: "fresh", linked_tools: ["claude-code"] },
    ]);
  });
});