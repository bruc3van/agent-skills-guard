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

/**
 * 已安装技能列表。
 *
 * `get_installed_skills` 并非纯读：它会遍历各工具 skill 目录、比对软链、
 * 必要时回写数据库，属于重量级调用。此前配置为 `staleTime: 0` +
 * `refetchOnMount: "always"`，导致每次切换标签页都触发一轮全盘 reconcile。
 *
 * 改为 30 秒 staleTime 后，标签页来回切换直接命中缓存；真正需要立即反映
 * 磁盘变化的场景（安装/卸载/同步工具链接、手动刷新、启动期 reconcile）
 * 都会显式 invalidate 或 refetch 这个 key，不依赖挂载时的自动重取。
 */
const INSTALLED_SKILLS_STALE_TIME_MS = 30 * 1000;

export function useInstalledSkills() {
  return useQuery({
    queryKey: ["skills", "installed"],
    queryFn: () => api.getInstalledSkills(),
    staleTime: INSTALLED_SKILLS_STALE_TIME_MS,
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
