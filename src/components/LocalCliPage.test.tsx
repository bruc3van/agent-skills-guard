// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
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
}));
vi.mock("../lib/api", () => ({
  api: {
    fetchLocalCliDescriptions,
  },
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

afterEach(() => {
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
});
