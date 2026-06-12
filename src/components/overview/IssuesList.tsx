import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  FolderOpen,
  Trash2,
  ChevronDown,
  ChevronUp,
  AlertTriangle,
  AlertCircle,
  Info,
  Eye,
  CheckCircle,
} from "lucide-react";
import type { SkillScanResult } from "@/types/security";
import type { Plugin } from "@/types";
import { SecurityDetailDialog } from "../SecurityDetailDialog";
import { api } from "@/lib/api";
import { appToast } from "@/lib/toast";

interface IssuesListProps {
  issues: Array<SkillScanResult & { kind: "skill" | "plugin"; local_path?: string }>;
  onOpenDirectory: (
    item: SkillScanResult & { kind: "skill" | "plugin"; local_path?: string }
  ) => void;
}

const levelConfig = {
  Critical: {
    color: "text-red-600",
    bg: "bg-red-500/10",
    iconBg: "bg-red-500",
    icon: AlertTriangle,
  },
  Medium: {
    color: "text-orange-600",
    bg: "bg-orange-500/10",
    iconBg: "bg-orange-500",
    icon: AlertCircle,
  },
  Safe: {
    color: "text-green-600",
    bg: "bg-green-500/10",
    iconBg: "bg-green-500",
    icon: Info,
  },
};

const mapSeverityTo3Levels = (severity: string): keyof typeof levelConfig => {
  if (severity === "Critical" || severity === "High") return "Critical";
  if (severity === "Medium" || severity === "Low") return "Medium";
  if (severity === "Info") return "Safe";
  return "Safe";
};

const normalizeLevel = (level: string): keyof typeof levelConfig => {
  if (level === "Critical" || level === "High") return "Critical";
  if (level === "Medium" || level === "Low") return "Medium";
  if (level === "Safe") return "Safe";
  return "Safe";
};

const getScoreColor = (score: number) => {
  if (score >= 90) return "text-green-600";
  if (score >= 70) return "text-orange-600";
  return "text-red-600";
};

export const IssuesList = React.memo(function IssuesList({ issues, onOpenDirectory }: IssuesListProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [expandedSkills, setExpandedSkills] = useState<Set<string>>(new Set());
  const [selectedSkill, setSelectedSkill] = useState<SkillScanResult | null>(null);

  const toggleExpanded = (skillId: string) => {
    setExpandedSkills((prev) => {
      const newSet = new Set(prev);
      if (newSet.has(skillId)) newSet.delete(skillId);
      else newSet.add(skillId);
      return newSet;
    });
  };

  const uninstallMutation = useMutation({
    mutationFn: async (skillId: string) => api.uninstallSkill(skillId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["skills", "installed"] });
      queryClient.invalidateQueries({ queryKey: ["skills"] });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
      appToast.success(t("skills.toast.uninstalled"), { duration: 3000 });
    },
    onError: (error: Error) => {
      appToast.error(t("skills.toast.uninstallFailed") + `: ${error.message}`, { duration: 4000 });
    },
  });

  const uninstallPluginMutation = useMutation({
    mutationFn: async (pluginId: string) => api.uninstallPlugin(pluginId),
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
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
      appToast.success(t("plugins.toast.uninstalled"), { duration: 3000 });
    },
    onError: (error: Error) => {
      appToast.error(t("plugins.toast.uninstallFailed") + `: ${error.message}`, {
        duration: 4000,
      });
    },
  });

  if (issues.length === 0) return null;

  return (
    <div className="divide-y divide-border/60">
      {issues.map((issue) => {
        const isExpanded = expandedSkills.has(issue.skill_id);
        const config = levelConfig[normalizeLevel(issue.level)];
        const LevelIcon = config.icon;

        const issueStats = issue.report.issues.reduce(
          (acc, item) => {
            const mappedSeverity = mapSeverityTo3Levels(item.severity);
            acc[mappedSeverity] = (acc[mappedSeverity] || 0) + 1;
            return acc;
          },
          {} as Record<string, number>
        );

        const uniqueIssues = Array.from(
          new Map(
            issue.report.issues.map((item) => [
              `${item.file_path || ""}::${item.description}`,
              item,
            ])
          ).values()
        );
        const topIssues = uniqueIssues
          .sort((a, b) => {
            const severityOrder: Record<string, number> = {
              Critical: 0,
              Error: 1,
              Warning: 2,
              Info: 3,
              High: 1,
              Medium: 2,
              Low: 3,
              Safe: 4,
            };
            return (severityOrder[a.severity] ?? 999) - (severityOrder[b.severity] ?? 999);
          })
          .slice(0, 3);

        return (
          <div key={issue.skill_id} className="px-5 py-5 hover:bg-secondary/30 transition-colors">
            <div className="flex flex-col md:flex-row md:items-center gap-4">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-3 flex-wrap">
                  <h3 className="font-semibold text-foreground">{issue.skill_name}</h3>
                  <span
                    className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium ${config.bg} ${config.color}`}
                  >
                    <LevelIcon className="w-3 h-3" strokeWidth={2.5} />
                    {t(`overview.riskLevels.${issue.level.toLowerCase()}`)}
                  </span>
                </div>
              </div>

              <div className="flex-shrink-0">
                <div className={`text-sm ${getScoreColor(issue.score)}`}>
                  <span className="text-muted-foreground">{t("skills.securityScore")}</span>
                  <span className="text-xl font-semibold ml-2">{issue.score}</span>
                </div>
              </div>

              <div className="flex gap-2">
                <button
                  onClick={() => onOpenDirectory(issue)}
                  className="apple-button-secondary h-8 px-3 text-xs flex items-center gap-1.5"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                  <span className="hidden sm:inline">{t("overview.issues.openDirectory")}</span>
                </button>
                <button
                  onClick={() =>
                    issue.kind === "skill"
                      ? uninstallMutation.mutate(issue.skill_id)
                      : uninstallPluginMutation.mutate(issue.skill_id)
                  }
                  disabled={uninstallMutation.isPending || uninstallPluginMutation.isPending}
                  className="apple-button-destructive h-8 px-3 text-xs flex items-center gap-1.5"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                  <span className="hidden sm:inline">{t("overview.issues.uninstall")}</span>
                </button>
              </div>
            </div>

            <div className="mt-4">
              {issue.report.issues.length === 0 ? (
                <div className="flex items-center gap-2 text-sm text-green-600">
                  <CheckCircle className="w-4 h-4" />
                  <span>{t("overview.issues.skillSafe")}</span>
                </div>
              ) : !isExpanded ? (
                <button
                  onClick={() => toggleExpanded(issue.skill_id)}
                  className="flex items-center justify-between w-full text-left text-sm text-muted-foreground hover:text-foreground transition-colors py-2 group"
                >
                  <span>
                    {t("overview.issues.found", {
                      count: issue.report.issues.length,
                      breakdown: Object.entries(issueStats)
                        .map(([severity, count]) =>
                          t("overview.issues.issueCount", {
                            count,
                            level: t(`security.levels.${severity.toLowerCase()}`),
                          })
                        )
                        .join("，"),
                    })}
                  </span>
                  <ChevronDown className="w-4 h-4 text-blue-500 group-hover:translate-y-0.5 transition-transform" />
                </button>
              ) : (
                <div className="space-y-3">
                  <button
                    onClick={() => toggleExpanded(issue.skill_id)}
                    className="flex items-center justify-between w-full text-left text-sm font-medium py-2 group"
                  >
                    <span>
                      {
                        t("overview.issues.found", {
                          count: issue.report.issues.length,
                          breakdown: "",
                        }).split("：")[0]
                      }
                    </span>
                    <ChevronUp className="w-4 h-4 text-blue-500 group-hover:-translate-y-0.5 transition-transform" />
                  </button>

                  {topIssues.map((item, idx) => {
                    const mappedSeverity = mapSeverityTo3Levels(item.severity);
                    const issueConfig = levelConfig[mappedSeverity] || levelConfig.Medium;
                    const IssueIcon = issueConfig.icon;

                    return (
                      <div
                        key={idx}
                        className="flex items-start gap-3 text-sm p-3 rounded-xl bg-secondary/50"
                      >
                        <div
                          className={`w-6 h-6 rounded-lg ${issueConfig.iconBg} flex items-center justify-center flex-shrink-0`}
                        >
                          <IssueIcon className="w-3.5 h-3.5 text-white" strokeWidth={2.5} />
                        </div>
                        <div className="flex-1 min-w-0">
                          <span className="text-foreground">
                            {item.file_path && (
                              <span className="text-blue-500 font-medium mr-1">
                                [{item.file_path}]
                              </span>
                            )}
                            {item.description}
                          </span>
                          {item.line_number && (
                            <span className="text-muted-foreground text-xs ml-2">
                              {t("overview.issues.lineNumber", { line: item.line_number })}
                            </span>
                          )}
                        </div>
                      </div>
                    );
                  })}

                  {issue.report.issues.length > 3 && (
                    <button
                      onClick={() => setSelectedSkill(issue)}
                      className="text-sm text-blue-500 hover:text-blue-600 font-medium flex items-center gap-1.5 transition-colors"
                    >
                      <span>{t("overview.issues.viewFullReport")}</span>
                      <Eye className="w-4 h-4" />
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
        );
      })}

      <SecurityDetailDialog
        result={selectedSkill}
        open={selectedSkill !== null}
        onClose={() => setSelectedSkill(null)}
      />
    </div>
  );
});
