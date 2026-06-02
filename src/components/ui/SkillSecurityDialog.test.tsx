// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SecurityReport } from "@/types/security";
import { SkillSecurityDialog, SkillSecurityDialogConfirmButton } from "./SkillSecurityDialog";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === "skills.installedPage.andMore") {
        return `还有 ${options?.count} 项`;
      }
      const labels: Record<string, string> = {
        "skills.marketplace.install.securityScore": "安全评分",
        "skills.marketplace.install.issuesDetected": "检测到问题",
        "skills.marketplace.install.critical": "严重",
        "skills.marketplace.install.highRisk": "高风险",
        "skills.marketplace.install.mediumRisk": "中风险",
        "skills.marketplace.install.warningTitle": "风险提示",
        "skills.marketplace.install.warningMessage": "请谨慎操作",
        "security.detail.lineNumber": "行号",
      };
      return labels[key] ?? key;
    },
  }),
}));

const report: SecurityReport = {
  skill_id: "agent-skills",
  score: 24,
  level: "Critical",
  blocked: false,
  hard_trigger_issues: [],
  scanned_files: [],
  partial_scan: false,
  skipped_files: [],
  recommendations: [],
  issues: [
    {
      severity: "Critical",
      category: "filesystem",
      file_path: ".opencode/skills",
      description: "SYMLINK: symbolic link detected inside skill directory",
    },
    {
      severity: "High",
      category: "command",
      file_path: "SKILL.md",
      description: "Command execution risk",
    },
  ],
};

afterEach(() => {
  cleanup();
});

describe("SkillSecurityDialog", () => {
  it("固定标题和底部操作区，仅中间安全详情区域滚动", () => {
    render(
      <SkillSecurityDialog
        open
        onOpenChange={vi.fn()}
        title="扫描结果"
        skillName="agent-skills"
        preparingLabel="准备安装"
        report={report}
        contentClassName="max-w-2xl max-h-[80vh]"
        footer={
          <SkillSecurityDialogConfirmButton loadingLabel="安装中" label="继续" onClick={vi.fn()} />
        }
      />
    );

    const dialog = screen.getByRole("alertdialog");
    expect(dialog.className).toContain("!overflow-hidden");
    expect(dialog.className).toContain("!flex");
    expect(dialog.className).toContain("!flex-col");
    expect(dialog.className).not.toContain("overflow-y-auto");

    expect(screen.getByTestId("skill-security-header").className).toContain("flex-shrink-0");
    expect(screen.getByTestId("skill-security-scroll-area").className).toContain("min-h-0");
    expect(screen.getByTestId("skill-security-scroll-area").className).toContain("overflow-y-auto");
    expect(screen.getByTestId("skill-security-footer").className).toContain("flex-shrink-0");
  });
});
