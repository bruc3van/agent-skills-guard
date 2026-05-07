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
    ],
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
}));

describe("ToolIcons", () => {
  it("always shows the universal folder button when the agents path is available", () => {
    render(
      <ToolIcons
        activeToolIds={[]}
        isLocalOnly
        onToggle={() => undefined}
      />
    );

    expect(screen.getByTitle("打开目录: C:/Users/Bruce/.agents/skills")).not.toBeNull();
  });
});
