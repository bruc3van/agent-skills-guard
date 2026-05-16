// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SkillScanResult } from "@/types/security";
import { SecurityDetailDialog } from "./SecurityDetailDialog";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const labels: Record<string, string> = {
        "security.detail.scanTime": "扫描时间",
        "security.detail.securityScore": "安全评分",
        "security.detail.recommendations": "建议",
        "security.detail.issues.critical": "严重问题",
        "security.detail.issues.high": "高风险问题",
        "security.detail.issues.medium": "中风险问题",
        "security.detail.issues.low": "低风险问题",
        "security.detail.close": "关闭",
        "security.detail.lineNumber": "行号",
      };
      return labels[key] ?? key;
    },
  }),
}));

const scanResult: SkillScanResult = {
  skill_id: "agent-skills",
  skill_name: "agent-skills",
  score: 24,
  level: "Critical",
  scanned_at: "2026-05-17T01:24:21.000+08:00",
  report: {
    skill_id: "agent-skills",
    score: 24,
    level: "Critical",
    blocked: true,
    hard_trigger_issues: [],
    scanned_files: [],
    partial_scan: false,
    skipped_files: [],
    recommendations: [
      "security.score_warning_severe",
      "security.recommendations.cmd_injection",
      "security.recommendations.secrets",
    ],
    issues: [
      {
        severity: "Critical",
        category: "filesystem",
        file_path: ".opencode/skills",
        description: "SYMLINK: symbolic link detected inside skill directory",
      },
      {
        severity: "Error",
        category: "command",
        file_path: "SKILL.md",
        description: "Command execution risk",
      },
      {
        severity: "Error",
        category: "secrets",
        file_path: "config.json",
        description: "Secret-like value detected",
      },
      {
        severity: "Info",
        category: "metadata",
        file_path: "README.md",
        description: "Low risk metadata issue",
      },
    ],
  },
};

afterEach(() => {
  cleanup();
});

describe("SecurityDetailDialog", () => {
  it("固定标题和底部操作区，仅中间详情区域滚动", () => {
    render(<SecurityDetailDialog result={scanResult} open onClose={vi.fn()} />);

    const dialog = screen.getByRole("alertdialog");
    expect(dialog.className).toContain("!overflow-hidden");
    expect(dialog.className).toContain("!flex");
    expect(dialog.className).toContain("!flex-col");

    expect(screen.getByTestId("security-detail-header").className).toContain("flex-shrink-0");
    expect(screen.getByTestId("security-detail-scroll-area").className).toContain("min-h-0");
    expect(screen.getByTestId("security-detail-scroll-area").className).toContain(
      "overflow-y-auto"
    );
    expect(screen.getByTestId("security-detail-footer").className).toContain("flex-shrink-0");
  });
});
