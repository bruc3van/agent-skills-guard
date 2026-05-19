import { useIsMutating, useQuery } from "@tanstack/react-query";

type RepositoriesStatus = {
  scanningRepoId: string | null;
  refreshingRepoId: string | null;
  deletingRepoId: string | null;
  preparingSkillId: string | null;
  installingSkillId: string | null;
  pendingSkillInstall: unknown | null;
};

type MarketplaceStatus = {
  preparingSkillId: string | null;
  installingSkillId: string | null;
  installingPluginId: string | null;
  pendingInstall: unknown | null;
  scanPromptPlugin: unknown | null;
};

export const REPOSITORIES_PAGE_STATUS_KEY = ["repositories", "page-status"] as const;
export const MARKETPLACE_INSTALL_STATUS_KEY = ["marketplace", "install-status"] as const;

export function useNavigationProtection() {
  const mutatingCount = useIsMutating();
  const { data: reposStatus } = useQuery<RepositoriesStatus>({
    queryKey: REPOSITORIES_PAGE_STATUS_KEY,
    queryFn: () => ({
      scanningRepoId: null,
      refreshingRepoId: null,
      deletingRepoId: null,
      preparingSkillId: null,
      installingSkillId: null,
      pendingSkillInstall: null,
    }),
    enabled: false,
    staleTime: Infinity,
    gcTime: Infinity,
  });
  const { data: marketStatus } = useQuery<MarketplaceStatus>({
    queryKey: MARKETPLACE_INSTALL_STATUS_KEY,
    queryFn: () => ({
      preparingSkillId: null,
      installingSkillId: null,
      installingPluginId: null,
      pendingInstall: null,
      scanPromptPlugin: null,
    }),
    enabled: false,
    staleTime: Infinity,
    gcTime: Infinity,
  });

  return {
    isBusy:
      mutatingCount > 0 ||
      reposStatus?.scanningRepoId != null ||
      reposStatus?.refreshingRepoId != null ||
      reposStatus?.deletingRepoId != null ||
      reposStatus?.preparingSkillId != null ||
      reposStatus?.installingSkillId != null ||
      reposStatus?.pendingSkillInstall != null ||
      marketStatus?.preparingSkillId != null ||
      marketStatus?.installingSkillId != null ||
      marketStatus?.installingPluginId != null ||
      marketStatus?.pendingInstall != null ||
      marketStatus?.scanPromptPlugin != null,
  };
}
