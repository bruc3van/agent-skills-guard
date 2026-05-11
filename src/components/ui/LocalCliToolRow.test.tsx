// @vitest-environment jsdom
import { render, screen, cleanup } from "@testing-library/react";
import { describe, it, expect, vi, afterEach } from "vitest";

afterEach(cleanup);
import { LocalCliToolRow } from "./LocalCliToolRow";
import type { LocalCliTool } from "../../types";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));

const base: LocalCliTool = {
  id: "bruce-doc-converter",
  detected_path: "/home/u/.local/bin/bruce-doc-converter",
  manager: "pip",
  update_available: false,
};

describe("LocalCliToolRow", () => {
  it("显示工具名", () => {
    render(<LocalCliToolRow tool={base} onUpdate={vi.fn()} isUpdating={false} />);
    expect(screen.getByText("bruce-doc-converter")).not.toBeNull();
  });

  it("有版本时显示版本", () => {
    render(
      <LocalCliToolRow
        tool={{ ...base, current_version: "0.3.1" }}
        onUpdate={vi.fn()}
        isUpdating={false}
      />
    );
    expect(screen.getByText(/0\.3\.1/)).not.toBeNull();
  });

  it("有更新时显示更新按钮", () => {
    render(
      <LocalCliToolRow
        tool={{ ...base, update_available: true, latest_version: "0.4.0" }}
        onUpdate={vi.fn()}
        isUpdating={false}
      />
    );
    expect(screen.getByRole("button")).not.toBeNull();
  });

  it("更新中时按钮禁用", () => {
    render(
      <LocalCliToolRow
        tool={{ ...base, update_available: true }}
        onUpdate={vi.fn()}
        isUpdating={true}
      />
    );
    const btn = screen.getByRole("button");
    expect(btn.hasAttribute("disabled")).toBe(true);
  });
});
