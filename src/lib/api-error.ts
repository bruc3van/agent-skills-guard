import { translateError } from "./error-codes";

/**
 * 结构化 API 错误类型。
 *
 * 对 Tauri invoke 抛出的原始错误进行统一包装，
 * 提供 code（错误码）和已翻译的 message。
 * 调用方可通过 `instanceof ApiError` 精确区分 API 错误与其他异常。
 */
export class ApiError extends Error {
  /** 原始错误码（如 GITHUB_RATE_LIMITED），可能为空 */
  public readonly code: string;
  /** 原始未翻译的错误消息 */
  public readonly rawMessage: string;

  constructor(rawMessage: string) {
    const translated = translateError(rawMessage);
    super(translated);
    this.name = "ApiError";
    this.rawMessage = rawMessage;
    // 尝试从原始消息中提取错误码
    const codeMatch = rawMessage.match(
      /^\[([A-Z][A-Z0-9_]+)\]|^([A-Z][A-Z0-9_]+)(?::|\s|$)/
    );
    this.code = codeMatch?.[1] ?? codeMatch?.[2] ?? "";
  }
}

/**
 * 安全调用 Tauri invoke，将异常统一包装为 ApiError。
 *
 * 现有调用方无需改动 —— ApiError 继承自 Error，
 * translateError / appToast 等消费方仍然兼容。
 */
export function safeInvoke<T>(promise: Promise<T>): Promise<T> {
  return promise.catch((err: unknown) => {
    const message =
      err instanceof Error ? err.message : err != null ? String(err) : "";
    throw new ApiError(message);
  });
}
