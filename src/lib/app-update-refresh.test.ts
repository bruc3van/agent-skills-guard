import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import {
  APP_SESSION_SKILL_REFRESH_KEY,
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
      return [];
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
  it("rescans local skills before refetching cached queries", async () => {
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

    const sessionStorage = createMemoryStorage();
    await refetchSkillStateAfterAppUpdate(queryClient, {
      sessionStorage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(scanCount).toBe(1);
    expect(calls.sort()).toEqual(["agentTools", "installed", "scanResults", "skills"]);
    expect(sessionStorage.getItem(APP_SESSION_SKILL_REFRESH_KEY)).toBe("1");
  });
});

describe("reconcileSkillStateOnAppStartup", () => {
  it("rescans once per session even when the app version is unchanged", async () => {
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
    const sessionStorage = createMemoryStorage();
    let scanCount = 0;
    calls.length = 0;

    const didRefresh = await reconcileSkillStateOnAppStartup(queryClient, "1.2.5", {
      storage,
      sessionStorage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(didRefresh).toBe(true);
    expect(scanCount).toBe(1);
    expect(calls.sort()).toEqual(["agentTools", "installed", "scanResults", "skills"]);
    expect(sessionStorage.getItem(APP_SESSION_SKILL_REFRESH_KEY)).toBe("1");

    const didRefreshAgain = await reconcileSkillStateOnAppStartup(queryClient, "1.2.5", {
      storage,
      sessionStorage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(didRefreshAgain).toBe(false);
    expect(scanCount).toBe(1);
  });

  it("rescans again when the app version changes within the same session", async () => {
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
      [APP_VERSION_SKILL_REFRESH_KEY]: "1.2.4",
    });
    const sessionStorage = createMemoryStorage({
      [APP_SESSION_SKILL_REFRESH_KEY]: "1",
    });
    let scanCount = 0;
    calls.length = 0;

    const didRefresh = await reconcileSkillStateOnAppStartup(queryClient, "1.2.5", {
      storage,
      sessionStorage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(didRefresh).toBe(true);
    expect(scanCount).toBe(1);
    expect(storage.getItem(APP_VERSION_SKILL_REFRESH_KEY)).toBe("1.2.5");
  });
});