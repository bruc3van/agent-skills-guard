import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { Plugin, PluginUninstallResult } from "../types";

type UsePluginsOptions = {
  mode?: "runtime" | "cached";
  enabled?: boolean;
};

export function pluginsQueryKey(lang: string) {
  return ["plugins", lang] as const;
}

export function pluginsCachedQueryKey(lang: string) {
  return ["plugins", lang, "cached"] as const;
}

export function usePlugins(options: UsePluginsOptions = {}) {
  const { i18n } = useTranslation();
  const mode = options.mode ?? "runtime";
  return useQuery({
    queryKey:
      mode === "runtime" ? pluginsQueryKey(i18n.language) : pluginsCachedQueryKey(i18n.language),
    queryFn: () => (mode === "runtime" ? api.getPlugins(i18n.language) : api.getPluginsCached()),
    enabled: options.enabled ?? true,
    staleTime: 10 * 60 * 1000,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    refetchOnMount: false,
  });
}

export function useClaudeMarketplaces() {
  return useQuery({
    queryKey: ["claudeMarketplaces"],
    queryFn: () => api.getClaudeMarketplaces(),
    staleTime: 10 * 60 * 1000,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    refetchOnMount: false,
  });
}

export function useUninstallPlugin() {
  const queryClient = useQueryClient();

  return useMutation<PluginUninstallResult, Error, string>({
    mutationFn: (pluginId: string) => api.uninstallPlugin(pluginId),
    onSuccess: (result, pluginId) => {
      if (result.success) {
        queryClient.setQueriesData<Plugin[]>({ queryKey: ["plugins"] }, (prev) => {
          if (!prev) return prev;
          return prev.map((plugin) =>
            plugin.id === pluginId
              ? {
                  ...plugin,
                  installed: false,
                  installed_at: undefined,
                  install_status: "uninstalled",
                }
              : plugin
          );
        });
      }
      queryClient.invalidateQueries({ queryKey: ["plugins"], refetchType: "active" });
    },
  });
}

export function useRemoveMarketplace() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      marketplaceName,
      marketplaceRepo,
    }: {
      marketplaceName: string;
      marketplaceRepo: string;
    }) => api.removeMarketplace(marketplaceName, marketplaceRepo),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plugins"] });
      queryClient.invalidateQueries({ queryKey: ["repositories"] });
      queryClient.invalidateQueries({ queryKey: ["claudeMarketplaces"] });
    },
  });
}
