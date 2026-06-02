import { describe, expect, it } from "vitest";
import { hasIssueMetadata } from "./security-utils";

describe("security-utils metadata helpers", () => {
  it("detects when issue metadata is present", () => {
    expect(hasIssueMetadata({ severity: "Info", category: "Other", description: "x" })).toBe(
      false
    );
    expect(
      hasIssueMetadata({
        severity: "Info",
        category: "Other",
        description: "x",
        remediation: "fix it",
      })
    ).toBe(true);
    expect(
      hasIssueMetadata({
        severity: "Info",
        category: "Other",
        description: "x",
        cwe_id: "CWE-94",
      })
    ).toBe(false);
  });
});