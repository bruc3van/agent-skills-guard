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

  it("passes a default allowPartialScan flag when preparing a skill installation", async () => {
    invokeMock.mockResolvedValueOnce({});

    await api.prepareSkillInstallation("skill-a", "zh");

    expect(invokeMock).toHaveBeenCalledWith("prepare_skill_installation", {
      skillId: "skill-a",
      locale: "zh",
      allowPartialScan: false,
    });
  });
});
