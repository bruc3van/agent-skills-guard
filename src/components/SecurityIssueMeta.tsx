import { useTranslation } from "react-i18next";
import type { SecurityIssue } from "@/types/security";
import { hasIssueMetadata } from "@/lib/security-utils";

interface SecurityIssueMetaProps {
  issue: SecurityIssue;
  compact?: boolean;
}

export function SecurityIssueMeta({ issue, compact = false }: SecurityIssueMetaProps) {
  const { t } = useTranslation();

  if (compact) {
    if (!issue.confidence) {
      return null;
    }
    return (
      <span className="ml-2 text-[11px] text-muted-foreground">
        ({t(`security.detail.confidence.${issue.confidence}`)})
      </span>
    );
  }

  if (!hasIssueMetadata(issue)) {
    return null;
  }

  const confidenceKey = issue.confidence
    ? `security.detail.confidence.${issue.confidence}`
    : null;

  return (
    <div className="mt-2 space-y-1.5 text-xs text-muted-foreground">
      {(issue.rule_id || issue.confidence) && (
        <div className="flex flex-wrap items-center gap-2">
          {issue.rule_id && (
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">
              {issue.rule_id}
            </span>
          )}
          {issue.confidence && confidenceKey && (
            <span
              className={`rounded px-1.5 py-0.5 ${
                issue.confidence === "High"
                  ? "bg-destructive/10 text-destructive"
                  : issue.confidence === "Medium"
                    ? "bg-warning/10 text-warning"
                    : "bg-primary/10 text-primary"
              }`}
            >
              {t("security.detail.confidenceLabel")}: {t(confidenceKey)}
            </span>
          )}
        </div>
      )}
      {issue.remediation && (
        <div>
          <span className="font-medium text-foreground">
            {t("security.detail.remediation")}:{" "}
          </span>
          {issue.remediation}
        </div>
      )}
    </div>
  );
}