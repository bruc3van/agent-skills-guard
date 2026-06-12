// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToolIcons } from "./ToolIcons";

vi.mock("@/lib/agent-tools", () => ({
  useAgentTools: () => ({
    data: [
      {
        id: "agents",
        label: "Universal (.agents)",
        path: "C:/Users/Bruce/.agents/skills",
        present: true,
        skill_count: 0,
      },
      {
        id: "codex",
        label: "Codex",
        path: "C:/Users/Bruce/.codex/skills",
        present: false,
        skill_count: 0,
      },
    ],
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (k: string, opts?: Record<string, unknown>) =>
      opts && opts.path ? `${k}:${opts.path}` : k,
    i18n: { language: "en" },
  }),
}));

describe("ToolIcons", () => {
  it("always shows the universal folder button when the agents path is available", () => {
    render(<ToolIcons activeToolIds={[]} isLocalOnly onToggle={() => undefined} />);

    expect(screen.getByTitle("tool_icons.open_dir:C:/Users/Bruce/.agents/skills")).not.toBeNull();
  });

  it("does not show a folder button for missing tool directories", () => {
    render(<ToolIcons activeToolIds={["codex"]} onToggle={() => undefined} />);

    expect(screen.queryByTitle("tool_icons.open_dir:C:/Users/Bruce/.codex/skills")).toBeNull();
  });
});
