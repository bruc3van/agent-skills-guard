import { describe, it, expect } from "vitest";
import { canNativeSelfUpdate, groupByManager, managerLabel } from "./local-cli";
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
    expect(managerLabel("native")).toBe("Native");
    expect(managerLabel("native", (key) => (key.endsWith("native") ? "原生安装" : key))).toBe(
      "原生安装"
    );
  });
  it("unknown 显示未知", () => {
    expect(managerLabel("unknown")).toBeTruthy();
  });
});

describe("canNativeSelfUpdate", () => {
  it("只开放已验证的原生自更新器", () => {
    expect(canNativeSelfUpdate(make("claude", "native"))).toBe(true);
    expect(canNativeSelfUpdate(make("codex", "native"))).toBe(true);
    expect(canNativeSelfUpdate(make("gemini", "native"))).toBe(false);
  });
});

describe("localCliQueryKey", () => {
  it("返回稳定键", () => {
    expect(localCliQueryKey()).toEqual(["local-cli-tools"]);
  });
});
