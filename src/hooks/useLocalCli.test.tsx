// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "../i18n/config";
import { LOCAL_CLI_QUERY_KEY, useRescanLocalCliTools, useUpdateLocalCliTool } from "./useLocalCli";

const mocks = vi.hoisted(() => ({
  listLocalCliTools: vi.fn(),
  rescanLocalCliTools: vi.fn(),
  updateLocalCliTool: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  api: {
    listLocalCliTools: mocks.listLocalCliTools,
    rescanLocalCliTools: mocks.rescanLocalCliTools,
    updateLocalCliTool: mocks.updateLocalCliTool,
  },
}));

vi.mock("../lib/toast", () => ({
  appToast: {
    success: vi.fn(),
    error: mocks.toastError,
  },
}));

function createWrapper(queryClient = new QueryClient()) {
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

const wrapper = createWrapper();

afterEach(() => {
  mocks.listLocalCliTools.mockReset();
  mocks.rescanLocalCliTools.mockReset();
  mocks.updateLocalCliTool.mockReset();
  mocks.toastError.mockReset();
});

describe("useUpdateLocalCliTool", () => {
  it("shows string errors from Tauri invoke instead of undefined", async () => {
    mocks.updateLocalCliTool.mockRejectedValue("pnpm install failed");
    const { result } = renderHook(() => useUpdateLocalCliTool(), { wrapper });

    act(() => {
      result.current.mutate({
        id: "pnpm",
        detected_path: "/usr/local/bin/pnpm",
        manager: "pnpm",
        current_version: undefined,
        latest_version: undefined,
        update_available: false,
        last_checked: undefined,
        update_status: undefined,
        update_log: undefined,
        package_name: undefined,
        description: undefined,
      });
    });

    await waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledWith("pnpm 更新失败: pnpm install failed");
    });
  });
});

describe("useRescanLocalCliTools", () => {
  it("calls backend scan and writes fresh data to query cache", async () => {
    const queryClient = new QueryClient();
    const tools = [
      {
        id: "ffmpeg",
        detected_path: "/opt/homebrew/bin/ffmpeg",
        manager: "brew",
        current_version: "8.1_1",
        update_available: false,
      },
    ];
    mocks.rescanLocalCliTools.mockResolvedValue(tools);
    const { result } = renderHook(() => useRescanLocalCliTools(), {
      wrapper: createWrapper(queryClient),
    });

    act(() => {
      result.current.mutate();
    });

    await waitFor(() => {
      expect(queryClient.getQueryData(LOCAL_CLI_QUERY_KEY)).toEqual(tools);
    });
  });
});
