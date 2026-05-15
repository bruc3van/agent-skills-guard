import i18next from "i18next";

const ERROR_CODE_PATTERN = /^([A-Z][A-Z0-9_]+)(?::\s*)?(.*)$/;

export function translateError(message: string): string {
  const match = message.match(ERROR_CODE_PATTERN);
  if (match) {
    const [, code, detail] = match;
    const translated = i18next.t(`errors.${code}`);
    if (translated !== `errors.${code}`) {
      return detail ? `${translated}: ${detail}` : translated;
    }
  }
  return message;
}
