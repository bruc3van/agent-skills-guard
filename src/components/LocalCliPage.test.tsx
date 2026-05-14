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
const refetchLocalCliTools = vi.fn();
const rescanLocalCliTools = vi.fn();
const checkLocalCliUpdates = vi.fn();
let isRescanning = false;
let isChecking = false;

vi.mock("../hooks/useLocalCli", () => ({
  useLocalCliTools: () => ({
    data: mockTools,
    isLoading: false,
    refetch: refetchLocalCliTools,
  }),
  useRescanLocalCliTools: () => ({
    mutate: rescanLocalCliTools,
    isPending: isRescanning,
  }),
  useCheckLocalCliUpdates: () => ({ mutate: checkLocalCliUpdates, isPending: isChecking }),
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
  refetchLocalCliTools.mockReset();
  rescanLocalCliTools.mockReset();
  checkLocalCliUpdates.mockReset();
  isRescanning = false;
  isChecking = false;
});

describe("LocalCliPage", () => {
  it("渲染工具名", async () => {
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });
    expect(screen.getByText("bruce-doc-converter")).not.toBeNull();
  });

  it("显示 pnpm 管理器筛选标签", async () => {
    mockTools = [
      {
        id: "mmdc",
        detected_path: "/Users/u/Library/pnpm/bin/mmdc",
        manager: "pnpm",
        current_version: "11.0.0",
        update_available: false,
      },
    ];
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    expect(screen.getByText("localCli.tabs.pnpm")).not.toBeNull();
  });

  it("工具列表变化后继续为新工具请求说明", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["/home/u/.local/bin/bdc"]);
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
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["/home/u/.local/bin/new-cli"]);
    });
  });

  it("逐一获取说明信息完成后刷新 CLI 列表", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([
      ["/home/u/.local/bin/bdc", "Bruce doc converter CLI"],
    ]);
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(refetchLocalCliTools).toHaveBeenCalled();
    });
  });

  it("列表刷新后没有缺失说明时清理正在获取说明的进度提示", async () => {
    fetchLocalCliDescriptions.mockImplementation(() => new Promise(() => {}));
    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(screen.getByText("localCli.busy.fetchingDesc")).not.toBeNull();
    });

    mockTools = [
      {
        id: "bruce-doc-converter",
        detected_path: "/home/u/.local/bin/bdc",
        manager: "pip",
        current_version: "0.3.1",
        update_available: false,
        description: "Bruce doc converter CLI",
      },
    ];
    rerender(<LocalCliPage />);

    expect(screen.queryByText("localCli.busy.fetchingDesc")).toBeNull();
  });

  it("点击重新扫描会触发强制刷新并允许重试说明获取", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    const user = userEvent.setup();
    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledTimes(1);
    });

    await user.click(screen.getByRole("button", { name: "localCli.rescan" }));
    expect(rescanLocalCliTools).toHaveBeenCalledTimes(1);

    rerender(<LocalCliPage />);

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledTimes(2);
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
    expect(openLocalCliFolder).toHaveBeenCalledWith("/home/u/.local/bin/bdc");

    await user.click(
      screen.getByRole("button", {
        name: "localCli.uninstall: bruce-doc-converter",
      })
    );

    expect(screen.getByText("localCli.uninstallDialog.title")).not.toBeNull();
  });

  it("检查更新时禁用重新扫描，重新扫描时禁用检查更新", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    isChecking = true;
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    expect(
      (screen.getByRole("button", { name: "localCli.rescan" }) as HTMLButtonElement).disabled
    ).toBe(true);

    cleanup();
    isChecking = false;
    isRescanning = true;
    render(<LocalCliPage />, { wrapper });

    expect(
      (screen.getByRole("button", { name: "localCli.checkUpdates" }) as HTMLButtonElement).disabled
    ).toBe(true);
  });
});
