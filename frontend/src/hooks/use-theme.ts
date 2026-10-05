import { useCallback, useEffect, useState } from "react";

export type ThemeMode = "system" | "light" | "dark" | "night";

export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "theme";

const MODES: ThemeMode[] = ["system", "light", "dark", "night"];

function readStoredTheme(): ThemeMode {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return stored === "light" || stored === "dark" || stored === "night"
      ? stored
      : "system";
  } catch {
    return "system";
  }
}

function systemPrefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function resolve(mode: ThemeMode): ResolvedTheme {
  if (mode === "dark" || mode === "night") return "dark";
  // "light" forces light; "system" follows the OS preference
  return mode === "light" ? "light" : systemPrefersDark() ? "dark" : "light";
}

/**
 * Theme state for the system / light / dark / night modes, `next-themes`-style:
 * persists the mode in `localStorage("theme")`, syncs the `.dark` (and
 * optionally `.night`) classes onto `<html>`, and follows the OS
 * `prefers-color-scheme` while in `system` mode.
 */
export function useTheme() {
  const [theme, setThemeState] = useState<ThemeMode>(readStoredTheme);

  const setTheme = useCallback((mode: ThemeMode) => {
    try {
      window.localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // persistence is best-effort; the mode still applies for this session
    }
    setThemeState(mode);
  }, []);

  const cycleTheme = useCallback(() => {
    setThemeState((current) => {
      const next = MODES[(MODES.indexOf(current) + 1) % MODES.length]!;
      try {
        window.localStorage.setItem(STORAGE_KEY, next);
      } catch {
        // ignore
      }
      return next;
    });
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const root = document.documentElement;
      const isDark =
        theme === "dark" ||
        theme === "night" ||
        (theme === "system" && media.matches);
      root.classList.toggle("dark", isDark);
      root.classList.toggle("night", theme === "night");
    };
    apply();
    if (theme !== "system") return;
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  const resolvedTheme: ResolvedTheme = resolve(theme);

  return { theme, setTheme, cycleTheme, resolvedTheme };
}