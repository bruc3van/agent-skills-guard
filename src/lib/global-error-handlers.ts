/**
 * 全局错误兜底。
 *
 * ErrorBoundary 只能捕获 React 渲染期的异常。发生在事件回调、定时器、
 * 未 await 的 Promise 里的错误会静默丢失——排查「界面突然没反应」这类问题时
 * 拿不到任何现场。这里统一收口到 console.error，Tauri 的 WebView 控制台
 * 与开发期日志都能看到。
 *
 * 刻意不弹 toast：这些多为后台任务的非致命失败，打断用户没有意义；
 * 真正需要用户感知的错误由各调用点自行提示。
 */

/**
 * 防止重复注册。
 *
 * 每次调用都会创建新的处理函数对象，`addEventListener` 无法据此去重，
 * 重复调用会导致同一个错误被记录多次。当前唯一调用点是 main.tsx 的模块顶层
 * （天然只执行一次），这个标志是给将来可能出现的第二个调用点兜底。
 *
 * 被拦截的调用会拿到一个空的 cleanup，因此不会误解除别人注册的监听器。
 */
let installed = false;

function describe(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export function installGlobalErrorHandlers(target: Window = window): () => void {
  if (installed) return () => {};
  installed = true;

  const onError = (event: ErrorEvent) => {
    console.error(
      "[GlobalError]",
      describe(event.error ?? event.message),
      `at ${event.filename}:${event.lineno}:${event.colno}`,
      event.error instanceof Error ? event.error.stack : undefined
    );
  };

  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    console.error(
      "[UnhandledRejection]",
      describe(event.reason),
      event.reason instanceof Error ? event.reason.stack : undefined
    );
  };

  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onUnhandledRejection);

  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onUnhandledRejection);
    installed = false;
  };
}
