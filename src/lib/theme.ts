/** Appearance: follow macOS, or force light/dark. Remembered on this Mac. */
export type Theme = "system" | "light" | "dark";
const KEY = "app.theme";

export function getTheme(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch { /* storage unavailable */ }
  return "system";
}

export function applyTheme(theme: Theme = getTheme()) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export function setTheme(theme: Theme) {
  try { localStorage.setItem(KEY, theme); } catch { /* still applied for this session */ }
  applyTheme(theme);
  window.dispatchEvent(new Event("themechange"));
}

/** Whether the app currently shows dark colours (forced, or following a dark system). */
export function isDark(): boolean {
  const t = getTheme();
  if (t !== "system") return t === "dark";
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}
