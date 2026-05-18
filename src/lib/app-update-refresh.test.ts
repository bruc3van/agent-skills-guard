import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { AGENT_TOOLS_KEY } from "./agent-tools";
import { refetchSkillStateAfterAppUpdate } from "./app-update-refresh";

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
});
