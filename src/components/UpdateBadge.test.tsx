// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UpdateBadge } from "./UpdateBadge";

const dismissUpdate = vi.fn();

vi.mock("../contexts/UpdateContext", () => ({
  useUpdate: () => ({
    hasUpdate: true,
    updateInfo: {
      currentVersion: "1.1.0",
      availableVersion: "1.2.0",
    },
    isDismissed: false,
    dismissUpdate,
  }),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => (key === "update.available" ? "发现新版本" : key) }),
}));

afterEach(() => {
  cleanup();
  dismissUpdate.mockReset();
});

describe("UpdateBadge", () => {
  it("clicks the update badge to open settings", async () => {
    const onOpenSettings = vi.fn();
    const user = userEvent.setup();

    render(<UpdateBadge onOpenSettings={onOpenSettings} />);

    await user.click(screen.getByRole("button", { name: "发现新版本: v1.2.0" }));

    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    expect(dismissUpdate).not.toHaveBeenCalled();
  });
});
