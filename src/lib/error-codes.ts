import i18next from "i18next";

const ERROR_CODE_PATTERN = /^([A-Z][A-Z0-9_]+)(?::\s*)?(.*)$/;
const EMBEDDED_ERROR_CODE_PATTERN = /(?:^|:\s*)([A-Z][A-Z0-9_]+)(?::\s*)?(.*)$/;
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

export function translateError(message: string): string {
  const match = message.match(ERROR_CODE_PATTERN) ?? message.match(EMBEDDED_ERROR_CODE_PATTERN);
  if (match) {
    const [, code, detail] = match;

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
  }
  return message;
}
