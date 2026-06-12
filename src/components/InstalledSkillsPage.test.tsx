// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";

vi.mock("../hooks/useSkills", () => ({
  useInstalledSkills: () => ({
    data: [
      {
        id: "test-skill",
        name: "Test Skill",
        version: "1.0.0",
        description: "A test skill",
        repository_owner: "testuser",
        repository_url: "https://github.com/testuser/test-skill",
        installed_paths: [{ path: "/home/u/.claude/skills/test-skill", tool_id: "claude-code" }],
      },
    ],
    isLoading: false,
  }),
  useUninstallSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: null,
  }),
  useUninstallSkillPath: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: null,
  }),
}));

vi.mock("../hooks/usePlugins", () => ({
  useClaudeMarketplaces: () => ({ data: [], isLoading: false }),
  usePlugins: () => ({ data: [], isLoading: false }),
  useUninstallPlugin: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: null,
  }),
  useRemoveMarketplace: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: null,
  }),
}));

vi.mock("../lib/api", () => ({
  api: {
    getFeaturedMarketplaces: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock("../lib/toast", () => ({
  appToast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k, i18n: { language: "en" } }) }));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
}));

vi.mock("@/lib/agent-tools", () => ({
  AGENT_TOOLS_KEY: ["agent-tools"],
  useSyncSkillToTools: () => ({ mutate: vi.fn(), isPending: false }),
  useSyncAllSkillsToTools: () => ({ mutate: vi.fn(), isPending: false }),
  useAgentTools: () => ({ data: [{ id: "claude-code", name: "Claude Code" }] }),
}));

vi.mock("@/lib/installed-skills", () => ({
  getDisplayedPluginToolIds: vi.fn(() => []),
  getDisplayedToolIds: vi.fn(() => ["claude-code"]),
  getOperationSkillIds: vi.fn(() => []),
  getVisibleInstalledPaths: vi.fn(() => ["/home/u/.claude/skills/test-skill"]),
  getVisiblePluginInstallPath: vi.fn(() => null),
  groupSkillsByName: vi.fn((skills: unknown[]) =>
    (skills as Array<{ id: string; name: string }>).map((s) => ({
      ...s,
      displayName: s.name,
      entries: [{ path: "/home/u/.claude/skills/test-skill", tool_id: "claude-code" }],
      hasUpdate: false,
    }))
  ),
  normalizeInstalledSkills: vi.fn((skills: unknown[]) => skills),
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

afterEach(() => {
  cleanup();
});

describe("InstalledSkillsPage", () => {
  it("renders without crashing", async () => {
    const { InstalledSkillsPage } = await import("./InstalledSkillsPage");
    render(<InstalledSkillsPage />, { wrapper });
    expect(screen.getByText("Test Skill")).not.toBeNull();
  });

  it("switches between tabs", async () => {
    const user = userEvent.setup();
    const { InstalledSkillsPage } = await import("./InstalledSkillsPage");
    render(<InstalledSkillsPage />, { wrapper });

    const skillsTab = screen.getByRole("button", { name: "installed.tabs.skills" });
    await user.click(skillsTab);

    const pluginsTab = screen.getByRole("button", { name: "installed.tabs.plugins" });
    await user.click(pluginsTab);

    const marketplacesTab = screen.getByRole("button", { name: "installed.tabs.marketplaces" });
    await user.click(marketplacesTab);

    const allTab = screen.getByRole("button", { name: "installed.tabs.all" });
    await user.click(allTab);
  });
});
