import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";
import type { LocalCliTool } from "../types";

export const LOCAL_CLI_QUERY_KEY = ["local-cli-tools"] as const;
export const localCliQueryKey = () => LOCAL_CLI_QUERY_KEY;

export function useLocalCliTools(opts: { enabled?: boolean } = {}) {
  return useQuery<LocalCliTool[]>({
    queryKey: LOCAL_CLI_QUERY_KEY,
    queryFn: () => api.listLocalCliTools(),
    staleTime: 60_000,
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

export function useUpdateLocalCliTool() {
  const qc = useQueryClient();
  return useMutation<string, Error, string>({
    mutationFn: (toolId) => api.updateLocalCliTool(toolId),
    onSuccess: (_log, toolId) => {
      qc.invalidateQueries({ queryKey: LOCAL_CLI_QUERY_KEY });
      appToast.success(`${toolId} 更新完成`);
    },
    onError: (err, toolId) => {
      appToast.error(`${toolId} 更新失败: ${err.message}`);
    },
  });
}

export function useUninstallLocalCliTool() {
  const qc = useQueryClient();
  return useMutation<string, Error, string>({
    mutationFn: (toolId) => api.uninstallLocalCliTool(toolId),
    onSuccess: (_log, toolId) => {
      qc.invalidateQueries({ queryKey: LOCAL_CLI_QUERY_KEY });
      appToast.success(`${toolId} 卸载完成`);
    },
    onError: (err, toolId) => {
      appToast.error(`${toolId} 卸载失败: ${err.message}`);
    },
  });
}
