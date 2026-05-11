import { describe, it, expect } from "vitest";
import { groupByManager, managerLabel } from "./local-cli";
import { localCliQueryKey } from "../hooks/useLocalCli";
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
  it("npm、pnpm 和 pip 只显示管理器名称", () => {
    expect(managerLabel("npm")).toBe("npm");
    expect(managerLabel("pnpm")).toBe("pnpm");
    expect(managerLabel("pip")).toBe("pip");
  });
  it("unknown 显示未知", () => {
    expect(managerLabel("unknown")).toBeTruthy();
  });
});

describe("localCliQueryKey", () => {
  it("返回稳定键", () => {
    expect(localCliQueryKey()).toEqual(["local-cli-tools"]);
  });
});
