// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useUpdateLocalCliTool } from "./useLocalCli";

const mocks = vi.hoisted(() => ({
  updateLocalCliTool: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  api: {
    updateLocalCliTool: mocks.updateLocalCliTool,
  },
}));

vi.mock("../lib/toast", () => ({
  appToast: {
    success: vi.fn(),
    error: mocks.toastError,
  },
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

afterEach(() => {
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
