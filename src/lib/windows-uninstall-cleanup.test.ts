import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

const repoRoot = path.resolve(__dirname, "..", "..");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
const cleanupFragmentPath = path.join(
  repoRoot,
  "src-tauri",
  "wix",
  "cleanup-app-data.wxs",
);

describe("Windows MSI uninstall cleanup", () => {
  it("wires an MSI fragment that removes only application-owned data directories", () => {
    const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));

    expect(tauriConfig.bundle.windows.wix.fragmentPaths).toContain(
      "wix/cleanup-app-data.wxs",
    );
    expect(existsSync(cleanupFragmentPath)).toBe(true);

    const cleanupFragment = readFileSync(cleanupFragmentPath, "utf8");

    expect(cleanupFragment).toContain("CleanupAppDataOnUninstall");
    expect(cleanupFragment).toContain('REMOVE="ALL" AND NOT UPGRADINGPRODUCTCODE');
    expect(cleanupFragment).toContain("com.agent-skills-guard.app");
    expect(cleanupFragment).toContain("agent-skills-guard");
    expect(cleanupFragment).toContain("$env:APPDATA");
    expect(cleanupFragment).toContain("$env:LOCALAPPDATA");
    expect(cleanupFragment).not.toContain(".agents");
    expect(cleanupFragment).not.toContain(".claude");
  });
});
