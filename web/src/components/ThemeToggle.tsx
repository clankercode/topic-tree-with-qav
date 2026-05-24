import { Moon, Sun, Monitor } from "lucide-react";
import { useThemeStore, type ThemeMode } from "../store/theme";

const icons: Record<ThemeMode, typeof Sun> = {
  light: Sun,
  dark: Moon,
  system: Monitor,
};

const labels: Record<ThemeMode, string> = {
  light: "Light",
  dark: "Dark",
  system: "System",
};

export function ThemeToggle() {
  const { mode, setMode } = useThemeStore();
  const Icon = icons[mode];

  const cycleMode = () => {
    const modes: ThemeMode[] = ["light", "dark", "system"];
    const idx = modes.indexOf(mode);
    setMode(modes[(idx + 1) % modes.length]);
  };

  return (
    <button
      onClick={cycleMode}
      className="flex items-center gap-1.5 rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-2.5 py-1.5 text-sm hover:bg-[rgb(var(--border))]"
      aria-label={`Theme: ${labels[mode]}`}
    >
      <Icon className="h-4 w-4" />
      <span className="hidden sm:inline">{labels[mode]}</span>
    </button>
  );
}
