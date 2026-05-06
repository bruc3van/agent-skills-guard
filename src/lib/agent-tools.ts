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
      // refetchType: 'active' 强制立即重新请求已挂载的查询，而不是仅标记为 stale
      // Tauri 桌面应用不会触发 window focus 事件，必须显式触发 refetch
      qc.invalidateQueries({ queryKey: ["skills"], refetchType: 'active' });
      qc.invalidateQueries({ queryKey: ["skills", "installed"], refetchType: 'active' });
      qc.invalidateQueries({ queryKey: AGENT_TOOLS_KEY, refetchType: 'active' });
    },
  });
}

export function useSyncAllSkillsToTools() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (tools: string[]) => api.syncAllSkillsToTools(tools),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["skills"], refetchType: 'active' });
      qc.invalidateQueries({ queryKey: ["skills", "installed"], refetchType: 'active' });
      qc.invalidateQueries({ queryKey: AGENT_TOOLS_KEY, refetchType: 'active' });
    },
  });
}
