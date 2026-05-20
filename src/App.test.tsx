import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function readLocale(locale: "zh" | "en") {
  return JSON.parse(
    readFileSync(resolve(process.cwd(), `src/i18n/locales/${locale}.json`), "utf8")
  ) as { common?: { pageLoading?: string } };
}

describe("PageFallback", () => {
  it("uses i18n instead of hardcoded loading text", () => {
    const source = readFileSync(resolve(process.cwd(), "src/App.tsx"), "utf8");

    expect(source).toContain('t("common.pageLoading")');
    expect(source).not.toContain("<span>加载中...</span>");
  });

  it("defines the page loading key in supported locales", () => {
    for (const locale of ["zh", "en"] as const) {
      const value = readLocale(locale).common?.pageLoading;

      expect(value).toEqual(expect.any(String));
      expect(value?.trim()).not.toBe("");
    }
  });
});
