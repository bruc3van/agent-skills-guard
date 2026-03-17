import { ReactNode, useMemo } from "react";
import { AlertTriangle, CheckCircle, Loader2, XCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SecurityReport } from "@/types/security";
import { countIssuesBySeverity, groupIssuesBySignature } from "@/lib/security-utils";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./alert-dialog";

interface SkillSecurityDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  skillName: string;
  preparingLabel: string;
  report: SecurityReport | null;
  issuePreviewCount?: number;
  contentClassName?: string;
  leadContent?: ReactNode;
  extraContent?: ReactNode;
  footer: ReactNode;
}

export function SkillSecurityDialog({
  open,
  onOpenChange,
  title,
  skillName,
  preparingLabel,
  report,
  issuePreviewCount = 3,
  contentClassName,
  leadContent,
  extraContent,
  footer,
}: SkillSecurityDialogProps) {
  const { t } = useTranslation();

  const isMediumRisk = report ? report.score >= 50 && report.score < 70 : false;
  const isHighRisk = report ? report.score < 50 || report.blocked : false;
  const skippedFiles = report?.skipped_files ?? [];
  const hasPartialScan = Boolean(report?.partial_scan || skippedFiles.length > 0);
  const skippedPreview = skippedFiles.slice(0, 5);

  const issueCounts = useMemo(
    () => (report ? countIssuesBySeverity(report.issues) : { critical: 0, error: 0, warning: 0 }),
    [report]
  );
  const groupedIssues = useMemo(() => (report ? groupIssuesBySignature(report.issues) : []), [report]);
  const previewGroups = useMemo(
    () => groupedIssues.slice(0, issuePreviewCount),
    [groupedIssues, issuePreviewCount]
  );

  if (!report) {
    return (
      <AlertDialog open={open} onOpenChange={onOpenChange}>
        <AlertDialogContent className={contentClassName}>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <Loader2 className="w-5 h-5 animate-spin" />
              {title}
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-4 pb-4">
                <div>
                  {preparingLabel}: <span className="font-semibold">{skillName}</span>
                </div>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>{footer}</AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    );
  }

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent className={contentClassName}>
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            {isHighRisk ? (
              <XCircle className="w-5 h-5 text-destructive" />
            ) : isMediumRisk ? (
              <AlertTriangle className="w-5 h-5 text-warning" />
            ) : (
              <CheckCircle className="w-5 h-5 text-success" />
            )}
            {title}
          </AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="space-y-4 pb-4">
              <div>
                {preparingLabel}: <span className="font-semibold">{skillName}</span>
              </div>

              <div className="flex items-center justify-between rounded-lg bg-muted/50 p-4">
                <span className="text-sm">{t("skills.marketplace.install.securityScore")}:</span>
                <span
                  className={`text-2xl font-semibold ${
                    report.score >= 70
                      ? "text-success"
                      : report.score >= 50
                        ? "text-warning"
                        : "text-destructive"
                  }`}
                >
                  {report.score}
                </span>
              </div>

              {leadContent}

              {hasPartialScan && (
                <div className="rounded-lg border border-warning/30 bg-warning/10 p-4 text-sm">
                  <div className="mb-1 font-medium text-warning">
                    {t("skills.marketplace.install.partialScanTitle")}
                  </div>
                  <div className="text-muted-foreground">
                    {t("skills.marketplace.install.partialScanDescription")}
                  </div>
                  <ul className="mt-3 space-y-1 text-xs text-warning">
                    {skippedPreview.length > 0 ? (
                      skippedPreview.map((file, index) => <li key={`${file}-${index}`}>• {file}</li>)
                    ) : (
                      <li>• {t("skills.marketplace.install.partialScanUnknown")}</li>
                    )}
                  </ul>
                  {skippedFiles.length > skippedPreview.length && (
                    <div className="mt-2 text-xs text-muted-foreground">
                      ...{" "}
                      {t("skills.installedPage.andMore", {
                        count: skippedFiles.length - skippedPreview.length,
                      })}
                    </div>
                  )}
                </div>
              )}

              {report.issues.length > 0 && (
                <div className="space-y-2">
                  <div className="text-sm font-medium">
                    {t("skills.marketplace.install.issuesDetected")}:
                  </div>
                  <div className="flex gap-4 text-sm">
                    {issueCounts.critical > 0 && (
                      <span className="text-destructive">
                        {t("skills.marketplace.install.critical")}: {issueCounts.critical}
                      </span>
                    )}
                    {issueCounts.error > 0 && (
                      <span className="text-warning">
                        {t("skills.marketplace.install.highRisk")}: {issueCounts.error}
                      </span>
                    )}
                    {issueCounts.warning > 0 && (
                      <span className="text-warning">
                        {t("skills.marketplace.install.mediumRisk")}: {issueCounts.warning}
                      </span>
                    )}
                  </div>
                </div>
              )}

              {report.issues.length > 0 && (
                <div
                  className={`rounded-lg p-3 ${
                    isHighRisk
                      ? "border border-destructive/30 bg-destructive/10"
                      : isMediumRisk
                        ? "border border-warning/30 bg-warning/10"
                        : "border border-success/30 bg-success/10"
                  }`}
                >
                  <div className="space-y-2 text-sm">
                    {previewGroups.map((group) =>
                      group.items.length === 1 ? (
                        <div key={group.key} className="text-xs">
                          {group.summary.file_path && (
                            <span className="mr-1.5 text-primary">[{group.summary.file_path}]</span>
                          )}
                          {group.summary.description}
                          {typeof group.summary.line_number === "number" && (
                            <span className="ml-2 text-muted-foreground">
                              ({t("security.detail.lineNumber")} {group.summary.line_number})
                            </span>
                          )}
                        </div>
                      ) : (
                        <details key={group.key} className="text-xs">
                          <summary className="flex cursor-pointer list-none items-center justify-between gap-2">
                            <span className="min-w-0 truncate">
                              {group.summary.file_path && (
                                <span className="mr-1.5 text-primary">
                                  [{group.summary.file_path}]
                                </span>
                              )}
                              {group.summary.description}
                            </span>
                            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-muted-foreground">
                              {group.items.length}
                            </span>
                          </summary>
                          <ul className="mt-2 space-y-1 border-l border-border/60 pl-3">
                            {group.items.map((item, itemIdx) => (
                              <li key={`${group.key}-${itemIdx}`} className="text-muted-foreground">
                                <span className="mr-1">#{itemIdx + 1}</span>
                                {typeof item.line_number === "number" && (
                                  <span className="mr-1">({t("security.detail.lineNumber")} {item.line_number})</span>
                                )}
                                {item.code_snippet && (
                                  <code className="font-mono text-[11px]">{item.code_snippet}</code>
                                )}
                              </li>
                            ))}
                          </ul>
                        </details>
                      )
                    )}
                    {groupedIssues.length > previewGroups.length && (
                      <div className="text-xs text-muted-foreground">
                        ...{" "}
                        {t("skills.installedPage.andMore", {
                          count: groupedIssues.length - previewGroups.length,
                        })}
                      </div>
                    )}
                  </div>
                </div>
              )}

              {isHighRisk && (
                <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-sm">
                  <div className="flex items-start gap-2">
                    <AlertTriangle className="mt-0.5 h-5 w-5 flex-shrink-0 text-destructive" />
                    <div>
                      <strong className="mb-1 block">
                        {t("skills.marketplace.install.warningTitle")}
                      </strong>
                      {report.blocked
                        ? t("skills.marketplace.install.blocked")
                        : t("skills.marketplace.install.warningMessage")}
                    </div>
                  </div>
                </div>
              )}

              {extraContent}
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>

        <AlertDialogFooter>{footer}</AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

interface SkillSecurityDialogConfirmButtonProps {
  disabled?: boolean;
  isLoading?: boolean;
  loadingLabel: string;
  label: string;
  onClick: () => void;
  tone?: "primary" | "warning" | "destructive" | "success";
}

const confirmButtonClassName: Record<NonNullable<SkillSecurityDialogConfirmButtonProps["tone"]>, string> =
  {
    primary: "apple-button-primary",
    warning: "bg-warning text-white hover:bg-warning/90",
    destructive: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
    success: "bg-success text-white hover:bg-success/90",
  };

export function SkillSecurityDialogConfirmButton({
  disabled,
  isLoading,
  loadingLabel,
  label,
  onClick,
  tone = "primary",
}: SkillSecurityDialogConfirmButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled || isLoading}
      className={`inline-flex h-10 items-center justify-center gap-2 rounded-md px-4 py-2 text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 ${confirmButtonClassName[tone]}`}
    >
      {isLoading ? (
        <>
          <Loader2 className="h-4 w-4 animate-spin" />
          {loadingLabel}
        </>
      ) : (
        label
      )}
    </button>
  );
}

export { AlertDialogCancel };
