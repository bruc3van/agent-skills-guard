import { useState } from "react";
import { useSkills, useInstallSkill, useUninstallSkill, useDeleteSkill } from "../hooks/useSkills";
import { Skill } from "../types";
import {
  Download,
  Trash2,
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  Package,
  Loader2,
  FolderOpen,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { appToast } from "../lib/toast";
import { formatAppDateTime } from "@/lib/locale";

export function SkillsPage() {
  const { t } = useTranslation();
  const { data: skills, isLoading } = useSkills();
  const installMutation = useInstallSkill();
  const uninstallMutation = useUninstallSkill();
  const deleteMutation = useDeleteSkill();

  const [filter, setFilter] = useState<"all" | "installed" | "not-installed">("all");
  const [installingSkillId, setInstallingSkillId] = useState<string | null>(null);
  const [uninstallingSkillId, setUninstallingSkillId] = useState<string | null>(null);
  const [deletingSkillId, setDeletingSkillId] = useState<string | null>(null);

  const filteredSkills = skills?.filter((skill) => {
    if (filter === "installed") return skill.installed;
    if (filter === "not-installed") return !skill.installed;
    return true;
  });

  const getSecurityBadge = (score?: number) => {
    if (!score) return null;

    if (score >= 90) {
      return (
        <span className="px-2 py-0.5 text-xs rounded-md border text-success border-success/30 bg-success/10">
          {t("skills.secure")}_{score}
        </span>
      );
    } else if (score >= 70) {
      return (
        <span className="px-2 py-0.5 text-xs rounded-md border text-warning border-warning/30 bg-warning/10">
          {t("skills.lowRisk")}_{score}
        </span>
      );
    } else if (score >= 50) {
      return (
        <span className="px-2 py-0.5 text-xs rounded-md border text-orange-500 border-orange-500/30 bg-orange-500/10">
          {t("skills.medRisk")}_{score}
        </span>
      );
    } else {
      return (
        <span className="px-2 py-0.5 text-xs rounded-md border text-destructive border-destructive/30 bg-destructive/10">
          {t("skills.highRisk")}_{score}
        </span>
      );
    }
  };

  return (
    <div className="space-y-6">
      {/* Header Section */}
      <div className="flex flex-col sm:flex-row sm:justify-between sm:items-center gap-4 pb-4 border-b border-border">
        <div>
          <h2 className="text-lg font-semibold text-foreground flex items-center gap-2">
            <Package className="w-5 h-5 text-primary" />
            <span>{t("skills.title")}</span>
          </h2>
          <p className="text-xs text-muted-foreground mt-1">
            {filteredSkills?.length || 0} {t("skills.totalEntries")}
          </p>
        </div>

        {/* Filter Buttons */}
        <div className="flex gap-2">
          <button
            onClick={() => setFilter("all")}
            className={`px-4 py-2 rounded-lg text-xs transition-all ${
              filter === "all"
                ? "bg-primary text-primary-foreground"
                : "bg-muted border border-border text-muted-foreground hover:border-primary hover:text-primary"
            }`}
          >
            {t("skills.all")} [{skills?.length || 0}]
          </button>
          <button
            onClick={() => setFilter("installed")}
            className={`px-4 py-2 rounded-lg text-xs transition-all ${
              filter === "installed"
                ? "bg-success text-white"
                : "bg-muted border border-border text-muted-foreground hover:border-success hover:text-success"
            }`}
          >
            {t("skills.installed")} [{skills?.filter((s) => s.installed).length || 0}]
          </button>
          <button
            onClick={() => setFilter("not-installed")}
            className={`px-4 py-2 rounded-lg text-xs transition-all ${
              filter === "not-installed"
                ? "bg-violet-500 text-white"
                : "bg-muted border border-border text-muted-foreground hover:border-violet-500 hover:text-violet-500"
            }`}
          >
            {t("skills.available")} [{skills?.filter((s) => !s.installed).length || 0}]
          </button>
        </div>
      </div>

      {/* Skills Grid */}
      {isLoading ? (
        <div className="flex flex-col items-center justify-center py-16">
          <Loader2 className="w-12 h-12 text-primary animate-spin mb-4" />
          <p className="text-sm text-primary">{t("skills.loading")}</p>
        </div>
      ) : filteredSkills && filteredSkills.length > 0 ? (
        <div className="grid gap-4">
          {filteredSkills.map((skill) => (
            <SkillCard
              key={skill.id}
              skill={skill}
              onInstall={(allowPartialScan?: boolean) => {
                setInstallingSkillId(skill.id);
                installMutation.mutate(
                  {
                    skillId: skill.id,
                    allowPartialScan: allowPartialScan ?? false,
                  },
                  {
                    onSuccess: () => {
                      setInstallingSkillId(null);
                      appToast.success(t("skills.toast.installed"));
                    },
                    onError: (error: any) => {
                      setInstallingSkillId(null);
                      appToast.error(
                        `${t("skills.toast.installFailed")}: ${error.message || error}`
                      );
                    },
                  }
                );
              }}
              onUninstall={() => {
                setUninstallingSkillId(skill.id);
                uninstallMutation.mutate(skill.id, {
                  onSuccess: () => {
                    setUninstallingSkillId(null);
                    appToast.success(t("skills.toast.uninstalled"));
                  },
                  onError: (error: any) => {
                    setUninstallingSkillId(null);
                    appToast.error(
                      `${t("skills.toast.uninstallFailed")}: ${error.message || error}`
                    );
                  },
                });
              }}
              onDelete={() => {
                setDeletingSkillId(skill.id);
                deleteMutation.mutate(skill.id, {
                  onSuccess: () => {
                    setDeletingSkillId(null);
                    appToast.success(t("skills.toast.deleted"));
                  },
                  onError: (error: any) => {
                    setDeletingSkillId(null);
                    appToast.error(`${t("skills.toast.deleteFailed")}: ${error.message || error}`);
                  },
                });
              }}
              isInstalling={installingSkillId === skill.id}
              isUninstalling={uninstallingSkillId === skill.id}
              isDeleting={deletingSkillId === skill.id}
              isAnyOperationPending={
                installMutation.isPending || uninstallMutation.isPending || deleteMutation.isPending
              }
              getSecurityBadge={getSecurityBadge}
              t={t}
            />
          ))}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center py-16 border border-dashed border-border rounded-lg">
          <div className="text-primary text-2xl mb-4">{t("skills.empty")}</div>
          <p className="text-sm text-muted-foreground">{t("skills.noSkillsFound")}</p>
          <p className="text-xs text-muted-foreground mt-2">{t("skills.navigateToRepo")}</p>
        </div>
      )}
    </div>
  );
}

interface SkillCardProps {
  skill: Skill;
  onInstall: (allowPartialScan?: boolean) => void;
  onUninstall: () => void;
  onDelete: () => void;
  isInstalling: boolean;
  isUninstalling: boolean;
  isDeleting: boolean;
  isAnyOperationPending: boolean;
  getSecurityBadge: (score?: number) => React.ReactNode;
  t: (key: string, options?: any) => string;
}

function SkillCard({
  skill,
  onInstall,
  onUninstall,
  onDelete,
  isInstalling,
  isUninstalling,
  isDeleting,
  isAnyOperationPending,
  getSecurityBadge,
  t,
}: SkillCardProps) {
  const { i18n } = useTranslation();
  const [showDetails, setShowDetails] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);

  const handleOpenFolder = async () => {
    if (!skill.local_path) return;

    try {
      try {
        await invoke("open_skill_directory", { localPath: skill.local_path });
      } catch {
        await openPath(skill.local_path);
      }
      appToast.success(t("skills.folder.opened"), { duration: 5000 });
    } catch (error: any) {
      appToast.error(t("skills.folder.openFailed", { error: error?.message || String(error) }), {
        duration: 5000,
      });
    }
  };

  const hasPartialScan = Boolean(
    skill.security_report?.partial_scan || skill.security_report?.skipped_files?.length
  );

  const handleInstallClick = () => {
    if (
      (skill.security_score != null && skill.security_score < 70) ||
      (skill.security_issues && skill.security_issues.length > 0) ||
      hasPartialScan
    ) {
      setShowConfirm(true);
    } else {
      onInstall();
    }
  };

  const confirmInstall = () => {
    setShowConfirm(false);
    onInstall(hasPartialScan);
  };

  return (
    <div className="macos-card p-6">
      {/* Top Bar */}
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1">
          {/* Skill Name with Status */}
          <div className="flex items-center gap-3 mb-2">
            <h3 className="text-lg font-semibold text-foreground">{skill.name}</h3>
            {skill.installed ? (
              <span className="px-2 py-0.5 text-xs rounded-md bg-success/10 text-success border border-success/30">
                {t("skills.installed")}
              </span>
            ) : isInstalling ? (
              <span className="px-2 py-0.5 text-xs rounded-md bg-primary/10 text-primary border border-primary/30">
                {t("skills.installing")}
              </span>
            ) : null}
          </div>

          {/* Security Badge & Score */}
          <div className="flex items-center gap-3 flex-wrap">
            {getSecurityBadge(skill.security_score)}
            {skill.security_score != null && (
              <span className="text-xs text-muted-foreground">
                {t("skills.score")}:{" "}
                <span className="text-primary">{skill.security_score}/100</span>
              </span>
            )}
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex gap-2 ml-4">
          {skill.installed ? (
            <button
              onClick={onUninstall}
              disabled={isAnyOperationPending}
              className="macos-button-destructive disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isUninstalling ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                t("skills.uninstall")
              )}
            </button>
          ) : (
            <button
              onClick={handleInstallClick}
              disabled={isAnyOperationPending}
              className="macos-button-primary disabled:opacity-50 disabled:cursor-not-allowed inline-flex items-center gap-2"
            >
              {isInstalling ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  {t("skills.installing")}
                </>
              ) : (
                <>
                  <Download className="w-4 h-4" />
                  {t("skills.install")}
                </>
              )}
            </button>
          )}

          <button
            onClick={onDelete}
            disabled={isAnyOperationPending}
            aria-label={`${t("common.delete")}: ${skill.name}`}
            title={`${t("common.delete")}: ${skill.name}`}
            className="px-3 py-2 rounded-lg border border-border bg-card text-muted-foreground hover:border-destructive hover:text-destructive transition-all disabled:opacity-50"
          >
            {isDeleting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Trash2 className="w-4 h-4" />
            )}
          </button>
        </div>
      </div>

      {/* Description */}
      <p className="text-sm text-muted-foreground mb-3">
        {skill.description || t("skills.noDescription")}
      </p>

      {/* Repository Info */}
      <div className="flex items-center gap-4 mb-3 text-xs">
        <span className="text-muted-foreground">
          <span className="text-success">{t("skills.repo")}</span>{" "}
          {skill.repository_url === "local" ? (
            <span className="text-muted-foreground">{skill.repository_url}</span>
          ) : (
            skill.repository_url.split("/").slice(-2).join("/")
          )}
        </span>
        <span className="text-muted-foreground">
          <span className="text-violet-500">{t("skills.path")}</span> {skill.file_path}
        </span>
      </div>

      {/* Details Toggle */}
      <button
        onClick={() => setShowDetails(!showDetails)}
        className="flex items-center gap-2 text-xs text-primary hover:text-primary/80 transition-colors mt-4"
      >
        {showDetails ? (
          <>
            <ChevronUp className="w-4 h-4" />
            {t("skills.collapseDetails")}
          </>
        ) : (
          <>
            <ChevronDown className="w-4 h-4" />
            {t("skills.expandDetails")}
          </>
        )}
      </button>

      {/* Details Panel */}
      {showDetails && (
        <div className="mt-4 p-4 bg-muted/50 border border-border rounded-lg space-y-3">
          <div className="text-xs">
            <p className="text-primary mb-1">{t("skills.fullRepository")}:</p>
            {skill.repository_url === "local" ? (
              <p className="text-muted-foreground">{skill.repository_url}</p>
            ) : (
              <p className="text-muted-foreground break-all">{skill.repository_url}</p>
            )}
          </div>
          {skill.version && <DetailItem label={t("skills.version")} value={skill.version} />}
          {skill.author && <DetailItem label={t("skills.author")} value={skill.author} />}
          {skill.local_path && (
            <div className="text-xs">
              <p className="text-primary mb-1">{t("skills.localPath")}:</p>
              <button
                onClick={handleOpenFolder}
                aria-label={`${t("skills.openFolder")}: ${skill.local_path}`}
                title={`${t("skills.openFolder")}: ${skill.local_path}`}
                className="text-muted-foreground break-all hover:text-primary transition-colors flex items-center gap-2"
              >
                <FolderOpen className="w-4 h-4 flex-shrink-0" />
                <span className="text-left">{skill.local_path}</span>
              </button>
            </div>
          )}

          {skill.security_score != null && (
            <div className="text-xs">
              <p className="text-primary mb-1">{t("skills.securityAnalysis")}</p>
              <p className="text-muted-foreground">
                {skill.security_score}/100 {skill.security_score >= 90 && t("skills.safe")}
                {skill.security_score >= 70 &&
                  skill.security_score < 90 &&
                  t("skills.lowRiskLabel")}
                {skill.security_score >= 50 &&
                  skill.security_score < 70 &&
                  t("skills.mediumRiskLabel")}
                {skill.security_score < 50 && t("skills.highRiskInstallNotRecommended")}
              </p>
            </div>
          )}

          {skill.security_issues && skill.security_issues.length > 0 && (
            <div className="text-xs">
              <p className="text-destructive mb-2">{t("skills.securityIssuesDetected")}</p>
              <div className="space-y-1 pl-4 border-l-2 border-destructive/30">
                {skill.security_issues.map((issue, idx) => (
                  <p key={idx} className="text-muted-foreground">
                    <span className="text-destructive">[{idx + 1}]</span>{" "}
                    {issue.file_path ? `[${issue.file_path}] ` : ""}{issue.severity}: {issue.description}
                  </p>
                ))}
              </div>
            </div>
          )}

          {skill.installed_at && (
            <DetailItem
              label={t("skills.installedAt")}
              value={formatAppDateTime(skill.installed_at, i18n.language)}
            />
          )}
        </div>
      )}

      {/* Risk Confirmation Dialog */}
      {showConfirm && (
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-card border border-orange-500/30 rounded-xl p-6 max-w-md w-full shadow-xl">
            <div className="flex items-start gap-4 mb-6">
              <AlertTriangle className="w-8 h-8 text-orange-500 flex-shrink-0" />
              <div>
                <h3 className="text-xl font-semibold text-orange-500 mb-2">
                  {t("skills.securityWarning")}
                </h3>
                <p className="text-sm text-muted-foreground">{t("skills.highRiskSkillDetected")}</p>
              </div>
            </div>

            {skill.security_score != null && (
              <div className="mb-4 p-3 bg-orange-500/10 border border-orange-500/30 rounded-lg">
                <p className="text-xs text-orange-500 mb-1">{t("skills.securityScore")}</p>
                <p className="text-sm text-foreground">
                  {skill.security_score}/100
                  {skill.security_score < 50 && ` ${t("skills.criticalRisk")}`}
                  {skill.security_score >= 50 &&
                    skill.security_score < 70 &&
                    ` ${t("skills.elevatedRisk")}`}
                </p>
              </div>
            )}

            {skill.security_issues && skill.security_issues.length > 0 && (
              <div className="mb-4 p-3 bg-muted border border-border rounded-lg max-h-40 overflow-y-auto">
                <p className="text-xs text-destructive mb-2">{t("skills.detectedIssues")}</p>
                <ul className="text-xs space-y-1">
                  {skill.security_issues.slice(0, 5).map((issue, idx) => (
                    <li key={idx} className="text-muted-foreground">
                      <span className="text-destructive">[{idx + 1}]</span>{" "}
                      {issue.file_path ? `[${issue.file_path}] ` : ""}{issue.severity}: {issue.description}
                    </li>
                  ))}
                  {skill.security_issues.length > 5 && (
                    <li className="text-muted-foreground italic">
                      ... +{skill.security_issues.length - 5} {t("skills.moreIssues")}
                    </li>
                  )}
                </ul>
              </div>
            )}

            {hasPartialScan && (
              <div className="mb-4 p-3 bg-warning/10 border border-warning/30 rounded-lg">
                <p className="text-xs text-warning font-medium mb-1">
                  {t("skills.marketplace.install.partialScanTitle")}
                </p>
                <p className="text-xs text-muted-foreground">
                  {t("skills.marketplace.install.partialScanDescription")}
                </p>
              </div>
            )}

            <p className="text-xs text-muted-foreground mb-6 p-3 bg-muted/50 rounded-lg border border-border">
              <span className="text-orange-500">[!]</span> {t("skills.installWarning")}
            </p>

            <div className="flex gap-3">
              <button
                onClick={() => setShowConfirm(false)}
                className="macos-button-secondary flex-1"
              >
                {t("skills.abort")}
              </button>
              <button
                onClick={confirmInstall}
                className="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white hover:bg-orange-600 transition-colors text-sm font-medium"
              >
                {t("skills.proceedAnyway")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function DetailItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-xs">
      <p className="text-primary mb-1">{label}:</p>
      <p className="text-muted-foreground break-all">{value}</p>
    </div>
  );
}
