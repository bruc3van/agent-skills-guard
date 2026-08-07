import { afterEach, describe, expect, it, vi } from "vitest";
import { installGlobalErrorHandlers } from "./global-error-handlers";

type Listener = (event: unknown) => void;

/** 最小化的 EventTarget 替身，用于观察监听器的注册与解绑 */
function createFakeWindow() {
  const listeners = new Map<string, Set<Listener>>();
  return {
    listeners,
    count(type: string) {
      return listeners.get(type)?.size ?? 0;
    },
    dispatch(type: string, event: unknown) {
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
    addEventListener(type: string, listener: Listener) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type)!.add(listener);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.get(type)?.delete(listener);
    },
  };
}

let cleanups: Array<() => void> = [];

function install(fake: ReturnType<typeof createFakeWindow>) {
  const cleanup = installGlobalErrorHandlers(fake as unknown as Window);
  cleanups.push(cleanup);
  return cleanup;
}

afterEach(() => {
  // 模块级 installed 标志需要复位，否则会串扰后续用例
  for (const cleanup of cleanups.splice(0).reverse()) cleanup();
  vi.restoreAllMocks();
});

describe("installGlobalErrorHandlers", () => {
  it("registers error and unhandledrejection listeners", () => {
    const fake = createFakeWindow();
    install(fake);

    expect(fake.count("error")).toBe(1);
    expect(fake.count("unhandledrejection")).toBe(1);
  });

  it("logs uncaught errors and unhandled rejections", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const fake = createFakeWindow();
    install(fake);

    fake.dispatch("error", {
      error: new Error("boom"),
      message: "boom",
      filename: "a.ts",
      lineno: 1,
      colno: 2,
    });
    fake.dispatch("unhandledrejection", { reason: new Error("rejected") });

    expect(consoleError).toHaveBeenCalledTimes(2);
    expect(consoleError.mock.calls[0][0]).toBe("[GlobalError]");
    expect(consoleError.mock.calls[0][1]).toContain("boom");
    expect(consoleError.mock.calls[1][0]).toBe("[UnhandledRejection]");
    expect(consoleError.mock.calls[1][1]).toContain("rejected");
  });

  // 每次调用都会新建处理函数，addEventListener 无法去重，
  // 没有这个保护同一个错误会被记录多次
  it("does not register twice, and the blocked call returns a no-op cleanup", () => {
    const fake = createFakeWindow();
    install(fake);
    const secondCleanup = install(fake);

    expect(fake.count("error")).toBe(1);

    // 被拦截的调用不得解除第一次注册的监听器
    secondCleanup();
    expect(fake.count("error")).toBe(1);
  });

  it("cleanup removes listeners and allows re-installing", () => {
    const fake = createFakeWindow();
    const cleanup = install(fake);

    cleanup();
    expect(fake.count("error")).toBe(0);
    expect(fake.count("unhandledrejection")).toBe(0);

    install(fake);
    expect(fake.count("error")).toBe(1);
  });

  it("describes non-Error rejection reasons without throwing", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const fake = createFakeWindow();
    install(fake);

    fake.dispatch("unhandledrejection", { reason: { code: "E_NOPE" } });
    fake.dispatch("unhandledrejection", { reason: "plain string" });

    expect(consoleError.mock.calls[0][1]).toContain("E_NOPE");
    expect(consoleError.mock.calls[1][1]).toBe("plain string");
  });
});
