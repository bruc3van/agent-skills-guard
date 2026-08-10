import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

describe("api", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("accepts the serialized Rust report shape when preparing a skill installation", async () => {
    invokeMock.mockResolvedValueOnce({
      skill_id: "skill-a",
      score: 100,
      level: "Safe",
      issues: [
        {
          severity: "Info",
          category: "Other",
          description: "No source location is available",
          line_number: null,
          code_snippet: null,
          file_path: null,
        },
      ],
      recommendations: [],
      blocked: false,
      hard_trigger_issues: [],
      partial_scan: false,
      skipped_files: [],
    });

    const report = await api.prepareSkillInstallation("skill-a", "zh");

    expect(report.scanned_files).toEqual([]);

    expect(invokeMock).toHaveBeenCalledWith("prepare_skill_installation", {
      skillId: "skill-a",
      locale: "zh",
    });
  });

  it("rejects malformed security reports from IPC", async () => {
    invokeMock.mockResolvedValueOnce({ skill_id: "skill-a", score: "100" });

    await expect(api.prepareSkillInstallation("skill-a", "zh")).rejects.toMatchObject({
      code: "IPC_RESPONSE_INVALID",
    });
  });
});
