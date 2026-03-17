const DEFAULT_LOCALE = "en-US";

// Only zh-CN and en-US are intentionally supported — they are the only UI
// languages the app ships with. All other language codes fall back to en-US.
// When a new language is added, extend this function accordingly.
export function resolveAppLocale(language?: string | null) {
  if (!language) return DEFAULT_LOCALE;
  return language.startsWith("zh") ? "zh-CN" : DEFAULT_LOCALE;
}

export function formatAppDate(
  value: Date | string,
  language?: string | null,
  options?: Intl.DateTimeFormatOptions
) {
  const date = typeof value === "string" ? new Date(value) : value;
  if (isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(resolveAppLocale(language), options).format(date);
}

export function formatAppDateTime(
  value: Date | string,
  language?: string | null,
  options?: Intl.DateTimeFormatOptions
) {
  return formatAppDate(value, language, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    ...options,
  });
}
