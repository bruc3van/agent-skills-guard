// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Skill } from "../types";
import { RepositoryPreviewDialog } from "./RepositoriesPage";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === "repositories.preview.title") {
        return `已添加仓库：${options?.name}`;
      }
      if (key === "repositories.preview.foundSummary") {
        return `技能 ${options?.skills}`;
      }
      const labels: Record<string, string> = {
        "repositories.preview.foundTitle": "发现以下内容：",
        "repositories.preview.goToMarket": "查看全部",
        "repositories.preview.close": "关闭",
        "skills.badge": "技能",
        "skills.install": "安装",
        "skills.scanning": "扫描中",
        "skills.noDescription": "暂无描述",
        "market.installed": "已安装",
      };
      return labels[key] ?? key;
    },
    i18n: { language: "zh" },
  }),
}));

const makeSkill = (id: string): Skill => ({
  id,
  name: id,
  description: `${id} description`,
  repository_url: "https://github.com/dontbesilent2025/dbskill",
  repository_owner: "dontbesilent2025",
  file_path: `${id}/SKILL.md`,
  installed: false,
});

afterEach(() => {
  cleanup();
});

describe("RepositoryPreviewDialog", () => {
  it("固定标题和底部操作区，仅中间内容区域滚动", () => {
    render(
      <RepositoryPreviewDialog
        preview={{
          repoName: "dontbesilent2025",
          repoUrl: "https://github.com/dontbesilent2025/dbskill",
          skills: Array.from({ length: 17 }, (_, index) => makeSkill(`dbs-${index + 1}`)),
        }}
        preparingSkillId={null}
        installingSkillId={null}
        onClose={vi.fn()}
        onNavigateToMarket={vi.fn()}
        onPrepareSkillInstall={vi.fn()}
      />
    );

    const dialog = screen.getByRole("alertdialog");
    expect(dialog.className).toContain("!overflow-hidden");
    expect(dialog.className).toContain("!flex");
    expect(dialog.className).toContain("!flex-col");

    expect(screen.getByTestId("repository-preview-header").className).toContain("flex-shrink-0");
    expect(screen.getByTestId("repository-preview-scroll-area").className).toContain("min-h-0");
    expect(screen.getByTestId("repository-preview-scroll-area").className).toContain(
      "overflow-y-auto"
    );
    expect(screen.getByTestId("repository-preview-footer").className).toContain("flex-shrink-0");
  });
});
