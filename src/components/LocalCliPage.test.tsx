// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";

vi.mock("../hooks/useLocalCli", () => ({
  useLocalCliTools: () => ({
    data: [
      {
        id: "bruce-doc-converter",
        detected_path: "/home/u/.local/bin/bdc",
        manager: "pip",
        current_version: "0.3.1",
        update_available: false,
      },
    ],
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
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));

const qc = new QueryClient();
const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={qc}>{children}</QueryClientProvider>
);

describe("LocalCliPage", () => {
  it("渲染工具名", async () => {
    const { LocalCliPage } = await import("./LocalCliPage");
    render(<LocalCliPage />, { wrapper });
    expect(screen.getByText("bruce-doc-converter")).not.toBeNull();
  });
});
