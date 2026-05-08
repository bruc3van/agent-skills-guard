import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogFooter,
  AlertDialogCancel,
} from "@/components/ui/alert-dialog";
import { Shield, AlertTriangle, Info } from "lucide-react";
import { useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { SkillScanResult, SecurityIssue } from "@/types/security";
import { sortIssuesBySeverity } from "@/lib/security-utils";

interface SecurityDetailDialogProps {
  result: SkillScanResult | null;
  open: boolean;
  onClose: () => void;
}

interface IssueGroup {
  key: string;
  summary: SecurityIssue;
  items: SecurityIssue[];
}

function groupIssuesForDisplay(issues: SecurityIssue[]): IssueGroup[] {
  const grouped = new Map<string, IssueGroup>();

  for (const issue of issues) {
    const key = `${issue.file_path ?? ""}::${issue.description}`;
    const existing = grouped.get(key);
    if (existing) {
      existing.items.push(issue);
      continue;
    }
    grouped.set(key, { key, summary: issue, items: [issue] });
  }

  return Array.from(grouped.values()).map((group) => ({
    ...group,
    items: [...group.items].sort((a, b) => (a.line_number ?? 0) - (b.line_number ?? 0)),
  }));
}

export function SecurityDetailDialog({ result, open, onClose }: SecurityDetailDialogProps) {
  const { t } = useTranslation();

  const { criticalIssues, highIssues, mediumIssues, lowIssues } = useMemo(() => {
    if (!result) {
      return {
        criticalIssues: [],
        highIssues: [],
        mediumIssues: [],
        lowIssues: [],
      };
    }

    const { report } = result;
    const sortedIssues = sortIssuesBySeverity(report.issues);
    return {
      criticalIssues: sortedIssues.filter((i) => i.severity === "Critical"),
      highIssues: sortedIssues.filter((i) => i.severity === "Error"),
      mediumIssues: sortedIssues.filter((i) => i.severity === "Warning"),
      lowIssues: sortedIssues.filter((i) => i.severity === "Info"),
    };
  }, [result]);

  if (!result) return null;

  const { report } = result;
  const skippedCount = report.skipped_files?.length ?? 0;
  const skippedPreview = skippedCount > 0 ? report.skipped_files.slice(0, 3).join(", ") : "";
  const showPartial = report.partial_scan || skippedCount > 0;

  return (
    <AlertDialog open={open} onOpenChange={onClose}>
      <AlertDialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-primary" />
            <div>
              <div className="text-xl">{result.skill_name}</div>
              <div className="text-sm text-muted-foreground font-normal mt-1">
                {t("security.detail.scanTime")}：{new Date(result.scanned_at).toLocaleString()}
              </div>
            </div>
          </AlertDialogTitle>
        </AlertDialogHeader>

        {/* 总体评分 */}
        <div className="flex items-center justify-between p-6 bg-muted/30 rounded-lg border border-border">
          <div>
            <div className="text-sm text-muted-foreground mb-1">
              {t("security.detail.securityScore")}
            </div>
            <div
              className={`text-5xl font-bold ${
                result.score >= 90
                  ? "text-success"
                  : result.score >= 70
                    ? "text-warning"
                    : result.score >= 50
                      ? "text-orange-500"
                      : "text-destructive"
              }`}
            >
              {result.score}
            </div>
          </div>
          <div>
            <span
              className={`px-4 py-2 rounded-lg text-lg font-medium border ${
                result.level === "Safe"
                  ? "bg-success/10 text-success border-success/30"
                  : result.level === "Low"
                    ? "bg-primary/10 text-primary border-primary/30"
                    : result.level === "Medium"
                      ? "bg-warning/10 text-warning border-warning/30"
                      : result.level === "High"
                        ? "bg-orange-500/10 text-orange-500 border-orange-500/30"
                        : "bg-destructive/10 text-destructive border-destructive/30"
              }`}
            >
              {result.level}
            </span>
          </div>
        </div>

        {showPartial && (
          <div className="p-4 bg-warning/10 border border-warning/30 rounded-lg text-sm">
            <div className="font-medium">{t("security.detail.partialScan")}</div>
            {skippedCount > 0 && (
              <div className="text-muted-foreground mt-1">
                {t("security.detail.skippedFiles", { count: skippedCount })}
                {skippedPreview && (
                  <div className="mt-1">
                    {t("security.detail.skippedExamples", { files: skippedPreview })}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {/* 问题列表 */}
        <div className="space-y-4">
          {criticalIssues.length > 0 && (
            <IssueSection
              title={t("security.detail.issues.critical")}
              icon={<AlertTriangle className="w-5 h-5 text-destructive" />}
              issues={criticalIssues}
              color="red"
            />
          )}

          {highIssues.length > 0 && (
            <IssueSection
              title={t("security.detail.issues.high")}
              icon={<AlertTriangle className="w-5 h-5 text-orange-500" />}
              issues={highIssues}
              color="orange"
              defaultCollapsed
            />
          )}

          {mediumIssues.length > 0 && (
            <IssueSection
              title={t("security.detail.issues.medium")}
              icon={<Info className="w-5 h-5 text-warning" />}
              issues={mediumIssues}
              color="yellow"
              defaultCollapsed
            />
          )}

          {lowIssues.length > 0 && (
            <IssueSection
              title={t("security.detail.issues.low")}
              icon={<Info className="w-5 h-5 text-primary" />}
              issues={lowIssues}
              color="blue"
              defaultCollapsed
            />
          )}
        </div>

        {/* 建议区域 */}
        {report.recommendations.length > 0 && (
          <div className="p-4 bg-warning/10 border border-warning/30 rounded-lg">
            <div className="text-sm font-medium mb-2">{t("security.detail.recommendations")}：</div>
            <ul className="space-y-1 text-sm">
              {report.recommendations.map((rec, idx) => (
                <li key={idx} className="flex items-start gap-2">
                  <span className="text-warning">▸</span>
                  <span>{rec}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel onClick={onClose}>{t("security.detail.close")}</AlertDialogCancel>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

// 问题区块组件
function IssueSection({
  title,
  icon,
  issues,
  color,
  defaultCollapsed = false,
}: {
  title: string;
  icon: React.ReactNode;
  issues: SecurityIssue[];
  color: "red" | "orange" | "yellow" | "blue";
  defaultCollapsed?: boolean;
}) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState(defaultCollapsed);
  const groupedIssues = useMemo(() => groupIssuesForDisplay(issues), [issues]);

  const colorClasses = {
    red: {
      border: "border-destructive/30",
      bg: "bg-destructive/5",
      hoverBg: "hover:bg-destructive/10",
    },
    orange: {
      border: "border-orange-500/30",
      bg: "bg-orange-500/5",
      hoverBg: "hover:bg-orange-500/10",
    },
    yellow: {
      border: "border-warning/30",
      bg: "bg-warning/5",
      hoverBg: "hover:bg-warning/10",
    },
    blue: {
      border: "border-primary/30",
      bg: "bg-primary/5",
      hoverBg: "hover:bg-primary/10",
    },
  };

  const classes = colorClasses[color];

  return (
    <div className={`border rounded-lg overflow-hidden ${classes.border}`}>
      <button
        onClick={() => setCollapsed(!collapsed)}
        className={`w-full flex items-center justify-between p-4 ${classes.bg} ${classes.hoverBg} transition-colors`}
      >
        <div className="flex items-center gap-2">
          {icon}
          <span className="font-medium">{title}</span>
          <span className="text-sm text-muted-foreground">({issues.length})</span>
        </div>
        <span className="text-sm text-muted-foreground">{collapsed ? "▼" : "▲"}</span>
      </button>

      {!collapsed && (
        <div className="p-4 space-y-3">
          {groupedIssues.map((group) =>
            group.items.length === 1 ? (
              <div key={group.key} className="p-3 bg-muted/30 rounded-lg border border-border">
                <div className="text-sm font-medium mb-2">
                  {group.summary.file_path && (
                    <span className="text-primary mr-2">[{group.summary.file_path}]</span>
                  )}
                  {group.summary.description}
                </div>
                {group.summary.code_snippet && (
                  <div className="mt-2">
                    {typeof group.summary.line_number === "number" && (
                      <div className="text-xs text-muted-foreground mb-1">
                        {t("security.detail.lineNumber")}：{group.summary.line_number}
                      </div>
                    )}
                    <pre className="p-2 bg-muted/50 rounded text-xs font-mono overflow-x-auto">
                      <code>{group.summary.code_snippet}</code>
                    </pre>
                  </div>
                )}
              </div>
            ) : (
              <details key={group.key} className="p-3 bg-muted/30 rounded-lg border border-border">
                <summary className="cursor-pointer list-none">
                  <div className="text-sm font-medium flex items-center gap-2">
                    <span>
                      {group.summary.file_path && (
                        <span className="text-primary mr-2">[{group.summary.file_path}]</span>
                      )}
                      {group.summary.description}
                    </span>
                    <span className="text-xs px-2 py-0.5 rounded bg-muted text-muted-foreground">
                      {group.items.length}
                    </span>
                  </div>
                </summary>
                <div className="mt-3 space-y-2 border-t border-border/60 pt-3">
                  {group.items.map((item, idx) => (
                    <div key={`${group.key}-${idx}`} className="p-2 bg-muted/50 rounded border border-border/70">
                      {typeof item.line_number === "number" && (
                        <div className="text-xs text-muted-foreground mb-1">
                          {t("security.detail.lineNumber")}：{item.line_number}
                        </div>
                      )}
                      {item.code_snippet && (
                        <pre className="p-2 bg-muted rounded text-xs font-mono overflow-x-auto">
                          <code>{item.code_snippet}</code>
                        </pre>
                      )}
                    </div>
                  ))}
                </div>
              </details>
            )
          )}
        </div>
      )}
    </div>
  );
}
