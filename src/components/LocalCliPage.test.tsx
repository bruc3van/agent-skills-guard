// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import type { LocalCliTool } from "../types";

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  warning: vi.fn(),
  error: vi.fn(),
}));

let mockTools: LocalCliTool[] = [
  {
    id: "bruce-doc-converter",
    detected_path: "/home/u/.local/bin/bdc",
    manager: "pip",
    current_version: "0.3.1",
    update_available: false,
    package_name: "bruce-doc-converter",
  },
];

const fetchLocalCliDescriptions = vi.fn();
const updateLocalCliTool = vi.fn();
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
    updateLocalCliTool,
    openLocalCliFolder,
    uninstallLocalCliTool,
  },
}));
vi.mock("../lib/toast", () => ({
  appToast: {
    success: toastMocks.success,
    warning: toastMocks.warning,
    error: toastMocks.error,
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
      package_name: "bruce-doc-converter",
    },
  ];
  fetchLocalCliDescriptions.mockReset();
  updateLocalCliTool.mockReset();
  openLocalCliFolder.mockReset();
  uninstallLocalCliTool.mockReset();
  uninstallMutation.mockReset();
  refetchLocalCliTools.mockReset();
  rescanLocalCliTools.mockReset();
  checkLocalCliUpdates.mockReset();
  toastMocks.success.mockReset();
  toastMocks.warning.mockReset();
  toastMocks.error.mockReset();
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

  // 中断的批次结果会被丢弃，必须允许重试；
  // 而已有结论的批次不能重试（见上一个用例），两者由 settled 标志区分。
  it("批次在拿到结果前被中断时，后续会重新请求这些工具", async () => {
    let resolveFirst: (value: Array<[string, string]>) => void = () => {};
    fetchLocalCliDescriptions.mockImplementationOnce(
      () =>
        new Promise<Array<[string, string]>>((resolve) => {
          resolveFirst = resolve;
        })
    );

    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["/home/u/.local/bin/bdc"]);
    });

    // 结果尚未返回就让 effect 重跑（模拟列表刷新）
    fetchLocalCliDescriptions.mockResolvedValue([]);
    mockTools = [...mockTools];
    rerender(<LocalCliPage />);

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledTimes(2);
    });
    // 被中断的工具重新进入请求
    expect(fetchLocalCliDescriptions).toHaveBeenLastCalledWith(["/home/u/.local/bin/bdc"]);

    // 迟到的结果不应造成异常
    await act(async () => {
      resolveFirst([["/home/u/.local/bin/bdc", "late result"]]);
    });
  });

  it("同名但路径不同的工具按 detected_path 分别缓存和重试说明", async () => {
    mockTools = [
      {
        id: "claude",
        detected_path: "/opt/homebrew/bin/claude",
        manager: "npm",
        current_version: "1.0.0",
        update_available: false,
      },
    ];
    fetchLocalCliDescriptions.mockImplementation(async ([path]: string[]) => [
      [path, path.includes("pnpm") ? "pnpm claude description" : "npm claude description"],
    ]);
    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(screen.getByText("npm claude description")).not.toBeNull();
    });

    mockTools = [
      ...mockTools,
      {
        id: "claude",
        detected_path: "/Users/u/Library/pnpm/bin/claude",
        manager: "pnpm",
        current_version: "1.0.0",
        update_available: false,
      },
    ];
    rerender(<LocalCliPage />);

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["/Users/u/Library/pnpm/bin/claude"]);
    });
    await waitFor(() => {
      expect(screen.getByText("pnpm claude description")).not.toBeNull();
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

  it("卸载后忽略仍在途的说明结果，且不再触发刷新", async () => {
    mockTools = [
      {
        id: "first-cli",
        detected_path: "/home/u/.local/bin/first-cli",
        manager: "pip",
        current_version: "1.0.0",
        update_available: false,
      },
      {
        id: "second-cli",
        detected_path: "/home/u/.local/bin/second-cli",
        manager: "pip",
        current_version: "1.0.0",
        update_available: false,
      },
    ];

    let resolveFirst: (value: Array<[string, string]>) => void = () => {};
    fetchLocalCliDescriptions.mockImplementation(
      () =>
        new Promise<Array<[string, string]>>((resolve) => {
          resolveFirst = resolve;
        })
    );

    const { LocalCliPage } = await import("./LocalCliPage");
    const { unmount } = render(<LocalCliPage />, { wrapper });

    // 缺描述的工具在一次调用里批量提交，而不是逐个串行发起
    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith([
        "/home/u/.local/bin/first-cli",
        "/home/u/.local/bin/second-cli",
      ]);
    });

    unmount();

    await act(async () => {
      resolveFirst([["/home/u/.local/bin/first-cli", "First CLI"]]);
    });

    expect(fetchLocalCliDescriptions).toHaveBeenCalledTimes(1);
    expect(refetchLocalCliTools).not.toHaveBeenCalled();
  });

  it("列表变化取消获取后忽略旧说明结果", async () => {
    mockTools = [
      {
        id: "first-cli",
        detected_path: "/home/u/.local/bin/first-cli",
        manager: "pip",
        current_version: "1.0.0",
        update_available: false,
      },
    ];

    let resolveFirst: (value: Array<[string, string]>) => void = () => {};
    fetchLocalCliDescriptions.mockImplementation(
      () =>
        new Promise<Array<[string, string]>>((resolve) => {
          resolveFirst = resolve;
        })
    );

    const { LocalCliPage } = await import("./LocalCliPage");
    const { rerender } = render(<LocalCliPage />, { wrapper });

    await waitFor(() => {
      expect(fetchLocalCliDescriptions).toHaveBeenCalledWith(["/home/u/.local/bin/first-cli"]);
    });

    mockTools = [];
    rerender(<LocalCliPage />);

    await act(async () => {
      resolveFirst([["/home/u/.local/bin/first-cli", "Stale CLI description"]]);
    });

    mockTools = [
      {
        id: "first-cli",
        detected_path: "/home/u/.local/bin/first-cli",
        manager: "pip",
        current_version: "1.0.0",
        update_available: false,
      },
    ];
    rerender(<LocalCliPage />);

    expect(screen.queryByText("Stale CLI description")).toBeNull();
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

  it("检查更新没有可更新工具时显示提示", async () => {
    fetchLocalCliDescriptions.mockResolvedValue([]);
    checkLocalCliUpdates.mockImplementation((_vars, options) => {
      options?.onSuccess?.([{ ...mockTools[0], update_available: false }]);
    });
    const user = userEvent.setup();
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    await user.click(screen.getByRole("button", { name: "localCli.checkUpdates" }));

    expect(toastMocks.success).toHaveBeenCalledWith("localCli.noUpdates");
  });

  it("部分工具检查失败时不误报全部最新", async () => {
    checkLocalCliUpdates.mockImplementation((_vars, options) => {
      options?.onSuccess?.([{ ...mockTools[0], update_check_error: "registry unavailable" }]);
    });
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: "localCli.checkUpdates" }));

    expect(toastMocks.warning).toHaveBeenCalledWith("localCli.partialCheckNoUpdates", {
      duration: 5000,
    });
    expect(toastMocks.success).not.toHaveBeenCalledWith("localCli.noUpdates");
  });

  it("原生自管理 CLI 不提供包管理器卸载按钮", async () => {
    mockTools = [
      {
        id: "claude",
        detected_path: "/home/u/.local/bin/claude",
        manager: "native",
        current_version: "2.1.223",
        update_available: false,
        package_name: "claude",
      },
    ];
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    expect(screen.getByText("localCli.tabs.native")).not.toBeNull();
    expect(screen.queryByRole("button", { name: "localCli.uninstall: claude" })).toBeNull();
    expect(screen.getByRole("button", { name: "localCli.card.checkAndUpdate" })).not.toBeNull();
  });

  it("在工具卡片展示具体的更新检查错误", async () => {
    mockTools = [{ ...mockTools[0], update_check_error: "registry unavailable" }];
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    const failure = screen.getByTitle("registry unavailable");
    expect(failure.textContent).toContain("localCli.card.checkFailed");
  });

  it("批量更新期间禁用检查更新", async () => {
    updateLocalCliTool.mockImplementation(() => new Promise(() => {}));
    mockTools = [
      {
        ...mockTools[0],
        latest_version: "0.4.0",
        update_available: true,
      },
    ];
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });

    await userEvent.click(screen.getByRole("button", { name: "localCli.updatesFocus.bulkUpdate" }));

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "localCli.checkUpdates" }).hasAttribute("disabled")
      ).toBe(true);
    });
  });
});
