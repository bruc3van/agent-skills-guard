// @vitest-environment jsdom

import { useEffect } from "react";
import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateProvider, useUpdate } from "./UpdateContext";
import { checkForUpdate, type UpdateHandle, type UpdateInfo } from "../lib/updater";

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
});
