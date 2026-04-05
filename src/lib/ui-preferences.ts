export type UiLanguage = "zh-CN" | "en-US";
export type UiTheme = "system" | "light" | "dark";

const UI_LANGUAGE_STORAGE_KEY = "nova.ui.language";
const UI_THEME_STORAGE_KEY = "nova.ui.theme";

export function normalizeUiLanguage(value: unknown): UiLanguage {
  const raw = typeof value === "string" ? value.trim().toLowerCase() : "";
  if (raw === "en" || raw === "en-us" || raw === "english") {
    return "en-US";
  }
  return "zh-CN";
}

export function normalizeUiTheme(value: unknown): UiTheme {
  const raw = typeof value === "string" ? value.trim().toLowerCase() : "";
  if (raw === "light" || raw === "dark") {
    return raw;
  }
  return "system";
}

export function getStoredUiLanguage(): UiLanguage {
  if (typeof window === "undefined") {
    return "zh-CN";
  }
  return normalizeUiLanguage(window.localStorage.getItem(UI_LANGUAGE_STORAGE_KEY));
}

export function setStoredUiLanguage(language: UiLanguage) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(UI_LANGUAGE_STORAGE_KEY, language);
}

export function getStoredUiTheme(): UiTheme {
  if (typeof window === "undefined") {
    return "system";
  }
  return normalizeUiTheme(window.localStorage.getItem(UI_THEME_STORAGE_KEY));
}

export function setStoredUiTheme(theme: UiTheme) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(UI_THEME_STORAGE_KEY, theme);
}

export function applyUiTheme(theme: UiTheme) {
  if (typeof document === "undefined") {
    return;
  }

  const prefersDark =
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;

  const shouldUseDark = theme === "dark" || (theme === "system" && prefersDark);
  document.documentElement.classList.toggle("dark", shouldUseDark);
}
