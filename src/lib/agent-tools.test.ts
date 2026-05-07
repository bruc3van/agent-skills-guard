import { describe, expect, it } from "vitest";
import { getDefaultInstallTargetToolIds } from "./agent-tools";
import type { AgentToolInfo } from "../types";

describe("getDefaultInstallTargetToolIds", () => {
  it("does not preselect Claude Code or other tool sync targets for skill installs", () => {
    const tools: AgentToolInfo[] = [
      {
        id: "agents",
        label: "Universal (.agents)",
        path: "C:/Users/Bruce/.agents/skills",
        present: true,
        skill_count: 0,
      },
      {
        id: "claude-code",
        label: "Claude Code",
        path: "C:/Users/Bruce/.claude/skills",
        present: true,
        skill_count: 0,
      },
      {
        id: "codex",
        label: "Codex",
        path: "C:/Users/Bruce/.codex/skills",
        present: true,
        skill_count: 0,
      },
    ];

    expect(getDefaultInstallTargetToolIds(tools)).toEqual([]);
  });
});
