import { describe, it, expect } from "vitest";
import { groupByManager, managerLabel } from "./local-cli";
import type { LocalCliTool } from "../types";

const make = (id: string, manager: string): LocalCliTool => ({
  id,
  detected_path: `/usr/bin/${id}`,
  manager,
  update_available: false,
});

describe("groupByManager", () => {
  it("将工具按包管理器分组", () => {
    const tools = [make("foo", "npm"), make("bar", "pip"), make("baz", "npm")];
    const groups = groupByManager(tools);
    expect(groups["npm"]?.length).toBe(2);
    expect(groups["pip"]?.length).toBe(1);
  });
});

describe("managerLabel", () => {
  it("npm 显示 npm（全局）", () => {
    expect(managerLabel("npm")).toContain("npm");
  });
  it("unknown 显示未知", () => {
    expect(managerLabel("unknown")).toBeTruthy();
  });
});
