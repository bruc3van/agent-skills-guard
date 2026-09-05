import { describe, expect, it } from "vitest";
import { hasIssueMetadata, scanReviewGroup } from "./security-utils";

describe("security-utils metadata helpers", () => {
  it("detects when issue metadata is present", () => {
    expect(hasIssueMetadata({ severity: "Info", category: "Other", description: "x" })).toBe(false);
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

describe("scan review groups", () => {
  it("keeps incomplete reports out of the safe count regardless of score", () => {
    expect(scanReviewGroup("Safe", { score: 100, partial_scan: true })).toBe("MidHigh");
    expect(scanReviewGroup("Low", { skipped_files: ["payload.pyc"] })).toBe("MidHigh");
    expect(scanReviewGroup("Safe", { blocked: true })).toBe("Severe");
    expect(scanReviewGroup("Critical", { partial_scan: true })).toBe("Severe");
    expect(scanReviewGroup("Safe", { score: 100, partial_scan: false })).toBe("Safe");
  });
});
