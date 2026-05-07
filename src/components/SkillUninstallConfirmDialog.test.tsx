// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SkillUninstallConfirmDialog } from "./SkillUninstallConfirmDialog";

describe("SkillUninstallConfirmDialog", () => {
  it("requires explicit confirmation before uninstalling all skill copies", async () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    const user = userEvent.setup();

    render(
      <SkillUninstallConfirmDialog
        open
        skillName="frontend-design"
        operationCount={2}
        pathCount={3}
        isConfirming={false}
        labels={{
          title: "Confirm uninstall",
          description: "This will uninstall all copies of frontend-design.",
          impact: "Records: 2, paths: 3",
          cancel: "Cancel",
          confirm: "Uninstall all",
          confirming: "Uninstalling",
        }}
        onCancel={onCancel}
        onConfirm={onConfirm}
      />
    );

    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByText("Confirm uninstall")).not.toBeNull();
    expect(screen.getByText("This will uninstall all copies of frontend-design.")).not.toBeNull();
    expect(screen.getByText("Records: 2, paths: 3")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "Uninstall all" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });
});
