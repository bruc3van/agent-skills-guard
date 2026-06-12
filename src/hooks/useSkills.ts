import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { appToast } from "../lib/toast";

export function useSkills() {
  return useQuery({
    queryKey: ["skills"],
    queryFn: () => api.getSkills(),
    staleTime: 5 * 60 * 1000,
  });
}

export function useInstalledSkills() {
  return useQuery({
    queryKey: ["skills", "installed"],
    queryFn: () => api.getInstalledSkills(),
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: false,
  });
}

interface InstallSkillVariables {
  skillId: string;
  installPath?: string;
  allowPartialScan?: boolean;
}

export function useInstallSkill() {
  const queryClient = useQueryClient();

  return useMutation<unknown, Error, InstallSkillVariables>({
    mutationFn: ({ skillId, installPath, allowPartialScan }) =>
      api.installSkill(skillId, installPath, allowPartialScan),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
    onError: (error: Error) => {
      console.error('Install skill failed:', error);
      appToast.error(error.message);
    },
  });
}

export function useUninstallSkill() {
  const queryClient = useQueryClient();

  return useMutation<unknown, Error, string>({
    mutationFn: (skillId: string) => api.uninstallSkill(skillId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
    onError: (error: Error) => {
      console.error('Uninstall skill failed:', error);
      appToast.error(error.message);
    },
  });
}

export function useUninstallSkillPath() {
  const queryClient = useQueryClient();

  return useMutation<unknown, Error, { skillId: string; path: string }>({
    mutationFn: ({ skillId, path }) => api.uninstallSkillPath(skillId, path),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
    onError: (error: Error) => {
      console.error('Uninstall skill path failed:', error);
      appToast.error(error.message);
    },
  });
}

export function useDeleteSkill() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (skillId: string) => api.deleteSkill(skillId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
    onError: (error: Error) => {
      console.error('Delete skill failed:', error);
      appToast.error(error.message);
    },
  });
}
