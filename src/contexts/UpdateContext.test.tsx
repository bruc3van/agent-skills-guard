// @vitest-environment jsdom

import { useEffect } from "react";
import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateProvider, useUpdate } from "./UpdateContext";
import { checkForUpdate, relaunchApp, type UpdateHandle, type UpdateInfo } from "../lib/updater";

vi.mock("../lib/updater", () => ({
  checkForUpdate: vi.fn(),
  relaunchApp: vi.fn(),
}));

vi.mock("../lib/platform", () => ({
  getPlatform: vi.fn(),
}));

vi.mock("../lib/rateLimit", () => ({
  isThrottleDue: vi.fn(() => false),
  markThrottleCompleted: vi.fn(),
}));

const localStorageMock = (() => {
  let store = new Map<string, string>();
  return {
    getItem: vi.fn((key: string) => store.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      store.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      store.delete(key);
    }),
    clear: vi.fn(() => {
      store = new Map();
    }),
  };
})();

Object.defineProperty(globalThis, "localStorage", {
  value: localStorageMock,
  configurable: true,
});

function Probe({ onReady }: { onReady: (ctx: ReturnType<typeof useUpdate>) => void }) {
  const ctx = useUpdate();

  useEffect(() => {
    onReady(ctx);
  }, [ctx, onReady]);

  return null;
}

describe("UpdateProvider", () => {
  beforeEach(() => {
    localStorageMock.clear();
    vi.mocked(checkForUpdate).mockReset();
    vi.mocked(relaunchApp).mockReset();
  });

  it("returns available update info from manual checks immediately", async () => {
    const info: UpdateInfo = {
      currentVersion: "1.1.3",
      availableVersion: "1.1.4",
      notes: "Bug fixes",
    };
    const update: UpdateHandle = {
      version: "1.1.4",
      downloadAndInstall: vi.fn(),
    };
    vi.mocked(checkForUpdate).mockResolvedValueOnce({
      status: "available",
      info,
      update,
    });

    let context: ReturnType<typeof useUpdate> | undefined;
    render(
      <UpdateProvider>
        <Probe
          onReady={(ctx) => {
            context = ctx;
          }}
        />
      </UpdateProvider>
    );

    let result: Awaited<ReturnType<ReturnType<typeof useUpdate>["checkUpdate"]>> | undefined;
    await act(async () => {
      result = await context!.checkUpdate();
    });

    expect(result).toEqual({ hasUpdate: true, info });
  });

  it("persists state before downloadAndInstall can terminate the Windows process", async () => {
    const calls: string[] = [];
    let finishDownload!: () => void;
    let markDownloadStarted!: () => void;
    const downloadStarted = new Promise<void>((resolve) => {
      markDownloadStarted = resolve;
    });
    const downloadFinished = new Promise<void>((resolve) => {
      finishDownload = resolve;
    });
    const info: UpdateInfo = {
      currentVersion: "1.3.5",
      availableVersion: "1.3.6",
    };
    const update: UpdateHandle = {
      version: "1.3.6",
      downloadAndInstall: vi.fn(() => {
        calls.push("downloadAndInstall");
        markDownloadStarted();
        return downloadFinished;
      }),
    };
    vi.mocked(checkForUpdate).mockResolvedValueOnce({
      status: "available",
      info,
      update,
    });
    vi.mocked(relaunchApp).mockImplementation(async () => {
      calls.push("relaunch");
    });

    let context: ReturnType<typeof useUpdate> | undefined;
    render(
      <UpdateProvider>
        <Probe
          onReady={(ctx) => {
            context = ctx;
          }}
        />
      </UpdateProvider>
    );

    await act(async () => {
      await context!.checkUpdate();
    });

    let installPromise!: Promise<boolean>;
    await act(async () => {
      installPromise = context!.installUpdate({
        onBeforeDownload: async () => {
          calls.push("persistSkillLinks");
        },
      });
      await downloadStarted;
    });

    // Windows exits from inside downloadAndInstall, so the required persistence
    // must already be complete while that Promise is still pending.
    expect(calls).toEqual(["persistSkillLinks", "downloadAndInstall"]);

    await act(async () => {
      finishDownload();
      await expect(installPromise).resolves.toBe(true);
    });

    expect(calls).toEqual(["persistSkillLinks", "downloadAndInstall", "relaunch"]);
  });
});
