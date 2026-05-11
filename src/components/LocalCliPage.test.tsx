// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import type { LocalCliTool } from "../types";

let mockTools: LocalCliTool[] = [
  {
    id: "bruce-doc-converter",
    detected_path: "/home/u/.local/bin/bdc",
    manager: "pip",
    current_version: "0.3.1",
    update_available: false,
  },
];

const fetchLocalCliDescriptions = vi.fn();
const openLocalCliFolder = vi.fn();
const uninstallLocalCliTool = vi.fn();
const uninstallMutation = vi.fn();

vi.mock("../hooks/useLocalCli", () => ({
  useLocalCliTools: () => ({
    data: mockTools,
    isLoading: false,
    refetch: vi.fn(),
  }),
  useCheckLocalCliUpdates: () => ({ mutate: vi.fn(), isPending: false }),
  useUpdateLocalCliTool: () => ({
    mutate: vi.fn(),
    isPending: false,
    variables: null,
  }),
  useUninstallLocalCliTool: () => ({
    mutateAsync: uninstallMutation,
    isPending: false,
    variables: null,
  }),
}));
vi.mock("../lib/api", () => ({
  api: {
    fetchLocalCliDescriptions,
    openLocalCliFolder,
    uninstallLocalCliTool,
  },
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

afterEach(() => {
  cleanup();
  mockTools = [
    {
      id: "bruce-doc-converter",
      detected_path: "/home/u/.local/bin/bdc",
      manager: "pip",
      current_version: "0.3.1",
      update_available: false,
    },
  ];
  fetchLocalCliDescriptions.mockReset();
  openLocalCliFolder.mockReset();
  uninstallLocalCliTool.mockReset();
  uninstallMutation.mockReset();
});

describe("LocalCliPage", () => {
  it("渲染工具名", async () => {
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });
    expect(screen.getByText("bruce-doc-converter")).not.toBeNull();
  });

  it("工具列表变化后继续为新工具请求说明", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["bruce-doc-converter"]);
    });

    mockTools = [
      ...mockTools,
      {
        id: "new-cli",
        detected_path: "/home/u/.local/bin/new-cli",
        manager: "pip",
        current_version: "1.0.0",
        update_available: false,
      },
    ];
    rerender(<LocalCliPage />);

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["new-cli"]);
    });
  });

  it("CLI 卡片提供打开文件夹和卸载确认操作", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    const user = userEvent.setup();
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    await user.click(
      screen.getByRole("button", {
        name: "localCli.card.openFolder: /home/u/.local/bin/bdc",
      })
    );
    expect(openLocalCliFolder).toHaveBeenCalledWith("bruce-doc-converter");

    await user.click(
      screen.getByRole("button", {
        name: "localCli.uninstall: bruce-doc-converter",
      })
    );

    expect(screen.getByText("localCli.uninstallDialog.title")).not.toBeNull();
  });
});
