import { create } from "zustand";

export type ThemeMode = "light" | "dark" | "system";

interface ThemeState {
  mode: ThemeMode;
  resolvedTheme: "light" | "dark";
  setMode(mode: ThemeMode): void;
  init(): void;
}

function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return mode;
}

function applyTheme(theme: "light" | "dark") {
  if (theme === "dark") {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
}

export const useThemeStore = create<ThemeState>((set) => ({
  mode: "system",
  resolvedTheme: "light",

  setMode(mode: ThemeMode) {
    const resolvedTheme = resolveTheme(mode);
    localStorage.setItem("theme", mode);
    applyTheme(resolvedTheme);
    set({ mode, resolvedTheme });
  },

  init() {
    const saved = localStorage.getItem("theme") as ThemeMode | null;
    const mode = saved || "system";
    const resolvedTheme = resolveTheme(mode);
    applyTheme(resolvedTheme);
    set({ mode, resolvedTheme });
  },
}));
