import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import { translateError } from "./error-codes";

describe("translateError", () => {
  beforeAll(async () => {
    if (!i18next.isInitialized) {
      await i18next.init({
        lng: "zh",
        resources: {
          zh: {
            translation: {
              errors: {
                LINK_CREATION_ALL_FAILED: "所有目标工具的链接创建均失败",
                PRIVATE_REPOSITORY_UNSUPPORTED: "私有仓库暂不支持获取",
              },
            },
          },
        },
      });
    }
  });

  it("translates known tool ids in error details", () => {
    expect(translateError("LINK_CREATION_ALL_FAILED: codex (permission denied)")).toBe(
      "所有目标工具的链接创建均失败: Codex (permission denied)"
    );
  });

  it("does not rewrite tool ids inside file paths", () => {
    expect(
      translateError(
        'LINK_CREATION_ALL_FAILED: codex (目标已存在同名但内容不同的技能: "/Users/bruce/.codex/skills/example")'
      )
    ).toBe(
      '所有目标工具的链接创建均失败: Codex (目标已存在同名但内容不同的技能: "/Users/bruce/.codex/skills/example")'
    );
  });

  it("translates private repository unsupported errors", () => {
    expect(translateError("PRIVATE_REPOSITORY_UNSUPPORTED")).toBe("私有仓库暂不支持获取");
  });

  it("translates private repository unsupported errors wrapped by backend context", () => {
    expect(translateError("下载仓库压缩包失败: PRIVATE_REPOSITORY_UNSUPPORTED")).toBe(
      "私有仓库暂不支持获取"
    );
  });
});
