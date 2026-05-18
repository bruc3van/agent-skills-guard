import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import {
  reconcileSkillStateAfterAppVersionChange,
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

describe("refetchSkillStateAfterAppUpdate", () => {
  it("refetches cached skill and tool queries even when they are inactive", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: Infinity,
        },
      },
    });
    const calls: string[] = [];

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

    calls.length = 0;

    await refetchSkillStateAfterAppUpdate(queryClient);

    expect(calls.sort()).toEqual(["agentTools", "installed", "scanResults", "skills"]);
  });

  it("rescans local skills and refetches state once after the app version changes", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: Infinity,
        },
      },
    });
    const calls: string[] = [];

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

    const storage = createMemoryStorage({
      "agent-skills-guard:skill-state-refreshed-version": "1.2.1",
    });
    let scanCount = 0;
    calls.length = 0;

    const didRefresh = await reconcileSkillStateAfterAppVersionChange(queryClient, "1.2.2", {
      storage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(didRefresh).toBe(true);
    expect(scanCount).toBe(1);
    expect(calls.sort()).toEqual(["agentTools", "installed", "scanResults", "skills"]);
    expect(storage.getItem("agent-skills-guard:skill-state-refreshed-version")).toBe("1.2.2");

    const didRefreshAgain = await reconcileSkillStateAfterAppVersionChange(queryClient, "1.2.2", {
      storage,
      scanLocalSkills: async () => {
        scanCount += 1;
        return [];
      },
    });

    expect(didRefreshAgain).toBe(false);
    expect(scanCount).toBe(1);
  });
});
