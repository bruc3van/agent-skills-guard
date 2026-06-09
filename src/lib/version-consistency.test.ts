import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function readJson(path: string) {
  return JSON.parse(readFileSync(resolve(path), "utf8"));
}

function readCargoPackageVersion(path: string) {
  const content = readFileSync(resolve(path), "utf8");
  const packageStart = content.indexOf("[package]");
  const packageTail = packageStart >= 0 ? content.slice(packageStart) : "";
  const nextSectionStart = packageTail.search(/\n\[/);
  const packageSection =
    nextSectionStart >= 0 ? packageTail.slice(0, nextSectionStart) : packageTail;
  return packageSection.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

function readCargoLockPackageVersion(path: string, packageName: string) {
  const content = readFileSync(resolve(path), "utf8");
  const sections = content.split(/\n(?=\[\[package\]\])/);

  for (const section of sections) {
    const name = section.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    if (name === packageName) {
      return section.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    }
  }

  return undefined;
}

describe("app version metadata", () => {
  it("keeps frontend, Tauri, Cargo package, and Cargo lock versions in sync", () => {
    const packageVersion = readJson("package.json").version;
    const tauriVersion = readJson("src-tauri/tauri.conf.json").version;
    const cargoVersion = readCargoPackageVersion("src-tauri/Cargo.toml");
    const cargoLockVersion = readCargoLockPackageVersion(
      "src-tauri/Cargo.lock",
      "agent-skills-guard"
    );

    expect(tauriVersion).toBe(packageVersion);
    expect(cargoVersion).toBe(packageVersion);
    expect(cargoLockVersion).toBe(packageVersion);
  });
});
