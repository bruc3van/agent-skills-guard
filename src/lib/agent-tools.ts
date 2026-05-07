import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import type { AgentToolInfo } from "../types";

export const AGENT_TOOLS_KEY = ["agent-tools"] as const;

export const TOOL_LABELS: Record<string, string> = {
  agents: "Universal (.agents)",
  "claude-code": "Claude Code",
  codex: "Codex",
  antigravity: "Antigravity",
  opencode: "OpenCode",
};

export function useAgentTools() {
  return useQuery<AgentToolInfo[]>({
    queryKey: AGENT_TOOLS_KEY,
    queryFn: () => api.listAgentTools(),
    staleTime: 30_000,
  });
}

export function useSyncSkillToTools() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ skillId, tools }: { skillId: string; tools: string[] }) =>
      api.syncSkillToTools(skillId, tools),
    onSuccess: () => {
      // refetchQueries 强制立即重新拉取，不受 staleTime 影响
      // Tauri 桌面应用不触发 window focus 事件，invalidateQueries 的后台调度可能延迟
      qc.refetchQueries({ queryKey: ["skills", "installed"] });
      qc.refetchQueries({ queryKey: ["skills"] });
      qc.refetchQueries({ queryKey: AGENT_TOOLS_KEY });
    },
  });
}

export function useSyncAllSkillsToTools() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (tools: string[]) => api.syncAllSkillsToTools(tools),
    onSuccess: () => {
      qc.refetchQueries({ queryKey: ["skills", "installed"] });
      qc.refetchQueries({ queryKey: ["skills"] });
      qc.refetchQueries({ queryKey: AGENT_TOOLS_KEY });
    },
  });
}
