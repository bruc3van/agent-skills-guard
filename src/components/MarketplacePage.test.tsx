// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";

if (typeof ResizeObserver === "undefined") {
  type ResizeObserverConstructor = new (
    callback: ResizeObserverCallback
  ) => ResizeObserver;
  const MockResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  (globalThis as unknown as { ResizeObserver: ResizeObserverConstructor }).ResizeObserver =
    MockResizeObserver as ResizeObserverConstructor;
}

vi.mock("../hooks/useSkills", () => ({
  useSkills: () => ({
    data: [
      {
        id: "test-skill",
        name: "Test Skill",
        version: "1.0.0",
        description: "A test skill",
        repository_owner: "testuser",
        repository_url: "https://github.com/testuser/test-skill",
      },
    ],
    isLoading: false,
  }),
  useInstallSkill: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: null,
  }),
}));

vi.mock("../hooks/usePlugins", () => ({
  usePlugins: () => ({
    data: [
      {
        id: "test-plugin",
        name: "Test Plugin",
        version: "1.0.0",
        description: "A test plugin",
        repository_owner: "testuser",
        discovery_source: "featured_marketplace",
      },
    ],
    isLoading: false,
  }),
}));

vi.mock("../lib/api", () => ({
  api: {
    cancelSkillInstallation: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock("../lib/toast", () => ({
  appToast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k, i18n: { language: "en" } }) }));

vi.mock("@/lib/storage", () => ({
  addRecentInstallPath: vi.fn(),
  getPluginScanPromptEnabled: vi.fn(() => true),
}));

vi.mock("@/lib/agent-tools", () => ({
  getDefaultInstallTargetToolIds: vi.fn(() => []),
  useAgentTools: () => ({ data: [{ id: "claude-code", name: "Claude Code" }] }),
}));

vi.mock("@/hooks/useNavigationProtection", () => ({
  MARKETPLACE_INSTALL_STATUS_KEY: ["marketplace", "install-status"],
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

afterEach(() => {
  cleanup();
});

describe("MarketplacePage", () => {
  it("renders without crashing", async () => {
    const { MarketplacePage } = await import("./MarketplacePage");
    render(<MarketplacePage />, { wrapper });
    expect(screen.getByText("Test Skill")).not.toBeNull();
    expect(screen.getByText("Test Plugin")).not.toBeNull();
  });

  it("switches between type tabs", async () => {
    const user = userEvent.setup();
    const { MarketplacePage } = await import("./MarketplacePage");
    render(<MarketplacePage />, { wrapper });

    const skillsTab = screen.getByRole("button", { name: "market.tabs.skills" });
    await user.click(skillsTab);

    const pluginsTab = screen.getByRole("button", { name: "market.tabs.plugins" });
    await user.click(pluginsTab);

    const allTab = screen.getByRole("button", { name: "market.tabs.all" });
    await user.click(allTab);
  });
});
