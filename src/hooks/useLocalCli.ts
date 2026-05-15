import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";
import type { LocalCliTool } from "../types";

export const LOCAL_CLI_QUERY_KEY = ["local-cli-tools"] as const;
export const localCliQueryKey = () => LOCAL_CLI_QUERY_KEY;

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object") {
    try {
      return JSON.stringify(error);
    } catch {
      // fall through
    }
  }
  return "未知错误";
}

export function useLocalCliTools(opts: { enabled?: boolean } = {}) {
  return useQuery<LocalCliTool[]>({
    queryKey: LOCAL_CLI_QUERY_KEY,
    queryFn: () => api.listLocalCliTools(),
    staleTime: 5 * 60_000,
    enabled: opts.enabled ?? true,
    refetchOnWindowFocus: false,
    refetchOnMount: false,
  });
}

export function useCheckLocalCliUpdates() {
  const qc = useQueryClient();
  return useMutation<LocalCliTool[], Error, void>({
    mutationFn: () => api.checkLocalCliUpdates(),
    onSuccess: (data) => qc.setQueryData(LOCAL_CLI_QUERY_KEY, data),
  });
}

export function useRescanLocalCliTools() {
  const qc = useQueryClient();
  return useMutation<LocalCliTool[], Error, void>({
    mutationFn: () => api.rescanLocalCliTools(),
    onSuccess: (data) => qc.setQueryData(LOCAL_CLI_QUERY_KEY, data),
  });
}

export function useUpdateLocalCliTool() {
  const qc = useQueryClient();
  return useMutation<string, unknown, LocalCliTool>({
    mutationFn: (tool) => api.updateLocalCliTool(tool.detected_path),
    onSuccess: (_log, tool) => {
      qc.setQueryData(LOCAL_CLI_QUERY_KEY, (old: LocalCliTool[] | undefined) => {
        if (!old) return old;
        return old.map((t) =>
          t.detected_path === tool.detected_path
            ? { ...t, update_available: false, update_status: "success" }
            : t
        );
      });
      qc.invalidateQueries({ queryKey: LOCAL_CLI_QUERY_KEY });
      appToast.success(`${tool.id} 更新完成`);
    },
    onError: (err, tool) => {
      appToast.error(`${tool.id} 更新失败: ${errorMessage(err)}`);
    },
  });
}

export function useUninstallLocalCliTool() {
  const qc = useQueryClient();
  return useMutation<string, unknown, LocalCliTool>({
    mutationFn: (tool) => api.uninstallLocalCliTool(tool.detected_path),
    onSuccess: (_log, tool) => {
      qc.invalidateQueries({ queryKey: LOCAL_CLI_QUERY_KEY });
      appToast.success(`${tool.id} 卸载完成`);
    },
    onError: (err, tool) => {
      appToast.error(`${tool.id} 卸载失败: ${errorMessage(err)}`);
    },
  });
}
