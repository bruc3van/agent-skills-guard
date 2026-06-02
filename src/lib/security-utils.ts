import type { SecurityIssue } from "@/types/security";

export function hasIssueMetadata(issue: SecurityIssue): boolean {
  return Boolean(issue.confidence || issue.remediation || issue.rule_id);
}

const severityOrder: Record<string, number> = {
  Critical: 0,
  Error: 1,
  Warning: 2,
  Info: 3,
};

export interface GroupedSecurityIssue {
  key: string;
  summary: SecurityIssue;
  items: SecurityIssue[];
}

export function countIssuesBySeverity(issues: SecurityIssue[]) {
  return {
    critical: issues.filter((i) => i.severity === "Critical").length,
    error: issues.filter((i) => i.severity === "Error").length,
    warning: issues.filter((i) => i.severity === "Warning").length,
    info: issues.filter((i) => i.severity === "Info").length,
  };
}

export function sortIssuesBySeverity(issues: SecurityIssue[]): SecurityIssue[] {
  return [...issues].sort((a, b) => {
    const orderA = severityOrder[a.severity] ?? 4;
    const orderB = severityOrder[b.severity] ?? 4;
    if (orderA !== orderB) return orderA - orderB;

    const fileA = a.file_path ?? "";
    const fileB = b.file_path ?? "";
    if (fileA !== fileB) return fileA.localeCompare(fileB);

    const lineA = a.line_number ?? Number.MAX_SAFE_INTEGER;
    const lineB = b.line_number ?? Number.MAX_SAFE_INTEGER;
    return lineA - lineB;
  });
}

export function groupIssuesBySignature(issues: SecurityIssue[]): GroupedSecurityIssue[] {
  const grouped = new Map<string, GroupedSecurityIssue>();

  for (const issue of sortIssuesBySeverity(issues)) {
    const key = `${issue.severity}::${issue.file_path ?? ""}::${issue.description}`;
    const existing = grouped.get(key);
    if (existing) {
      existing.items.push(issue);
      continue;
    }
    grouped.set(key, {
      key,
      summary: issue,
      items: [issue],
    });
  }

  return Array.from(grouped.values());
}
