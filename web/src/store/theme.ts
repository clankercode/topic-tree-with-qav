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
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
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

let systemMediaListenerAttached = false;

export const useThemeStore = create<ThemeState>((set, get) => ({
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
    if (
      !systemMediaListenerAttached &&
      typeof window !== "undefined" &&
      window.matchMedia
    ) {
      const mql = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = () => {
        if (get().mode !== "system") return;
        const next = mql.matches ? "dark" : "light";
        applyTheme(next);
        set({ resolvedTheme: next });
      };
      if (mql.addEventListener) {
        mql.addEventListener("change", handler);
      } else if ("addListener" in mql) {
        (mql as MediaQueryList).addListener(handler);
      }
      systemMediaListenerAttached = true;
    }
  },
}));
