import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";

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
    staleTime: 5 * 60 * 1000,
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
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
  });
}

export function useUninstallSkill() {
  const queryClient = useQueryClient();

  return useMutation<unknown, Error, string>({
    mutationFn: (skillId: string) => api.uninstallSkill(skillId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
  });
}

export function useUninstallSkillPath() {
  const queryClient = useQueryClient();

  return useMutation<unknown, Error, { skillId: string; path: string }>({
    mutationFn: ({ skillId, path }) => api.uninstallSkillPath(skillId, path),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
  });
}

export function useDeleteSkill() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (skillId: string) => api.deleteSkill(skillId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
    },
  });
}
