import i18next from "i18next";

const ERROR_CODE_PATTERN = /^([A-Z][A-Z0-9_]+)(?::\s*)?(.*)$/;
const EMBEDDED_ERROR_CODE_PATTERN = /(?:^|:\s*)([A-Z][A-Z0-9_]+)(?::\s*)?(.*)$/;
const BRACKETED_ERROR_CODE_PATTERN = /^\[([A-Z][A-Z0-9_]+)\]\s*(.*)$/;
const TOOL_LABELS: Record<string, string> = {
  agents: "Universal (.agents)",
  "claude-code": "Claude Code",
  codex: "Codex",
  antigravity: "Antigravity",
  opencode: "OpenCode",
};

function formatDetail(detail: string): string {
  return detail.replace(
    /(^|[;,]\s*)(agents|claude-code|codex|antigravity|opencode)(?=\s*(?:\(|,|;|$))/g,
    (_match, prefix: string, toolId: string) => {
      return `${prefix}${TOOL_LABELS[toolId] || toolId}`;
    }
  );
}

function tryTranslateCode(code: string, detail: string): string | null {
  if (code === "GITHUB_RATE_LIMITED" && detail && /^\d+$/.test(detail.trim())) {
    const withWait = i18next.t("errors.GITHUB_RATE_LIMITED_WITH_WAIT", {
      minutes: detail.trim(),
    });
    if (withWait !== "errors.GITHUB_RATE_LIMITED_WITH_WAIT") {
      return withWait;
    }
  }

  const translated = i18next.t(`errors.${code}`);
  if (translated !== `errors.${code}`) {
    return detail ? `${translated}: ${formatDetail(detail)}` : translated;
  }
  return null;
}

export function translateError(message: string): string {
  // Try [ERROR_CODE] message format
  const bracketMatch = message.match(BRACKETED_ERROR_CODE_PATTERN);
  if (bracketMatch) {
    const [, code, detail] = bracketMatch;
    const translated = tryTranslateCode(code, detail);
    if (translated) return translated;
  }

  // Try ERROR_CODE: message or ERROR_CODE message format
  const match = message.match(ERROR_CODE_PATTERN) ?? message.match(EMBEDDED_ERROR_CODE_PATTERN);
  if (match) {
    const [, code, detail] = match;
    const translated = tryTranslateCode(code, detail);
    if (translated) return translated;
  }

  // Fallback: return a generic error message if the raw message looks like a technical error
  if (message.length > 0) {
    return i18next.t("errors.GENERIC_ERROR", message);
  }
  return message;
}
