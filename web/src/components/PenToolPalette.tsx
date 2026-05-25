import { useState } from "react";
import { sendWsMsg } from "../ws/manager";
import {
  PEN_INK_INVERSE,
  PEN_INK_PRIMARY,
  resolvePenColor,
} from "../lib/penInk";
import { useThemeStore } from "../store/theme";

const CHROMATIC_COLORS = [
  "#ef4444",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#a855f7",
];

export type ToolMode = "pen" | "text";

export interface PenToolPaletteProps {
  boardId: string;
  color: string;
  size: number;
  tool: ToolMode;
  onColorChange: (color: string) => void;
  onSizeChange: (size: number) => void;
  onToolChange: (tool: ToolMode) => void;
  onUndo?: () => void;
  onClear?: () => void;
}

export function PenToolPalette({
  boardId,
  color,
  size,
  tool,
  onColorChange,
  onSizeChange,
  onToolChange,
  onUndo,
  onClear,
}: PenToolPaletteProps) {
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const isDark = useThemeStore((s) => s.resolvedTheme === "dark");
  const presetColors = [PEN_INK_PRIMARY, PEN_INK_INVERSE, ...CHROMATIC_COLORS];

  const handleUndo = () => {
    sendWsMsg({ v: 1, type: "PenUndo", boardId });
    onUndo?.();
  };

  const handleClear = () => {
    if (!showClearConfirm) {
      setShowClearConfirm(true);
      return;
    }
    sendWsMsg({ v: 1, type: "PenClear", boardId });
    onClear?.();
    setShowClearConfirm(false);
  };

  return (
    <div className="flex items-center gap-3 p-2 bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded">
      <div className="flex items-center gap-1">
        <span className="text-xs text-[rgb(var(--muted))]">Color</span>
        <div className="flex gap-1">
          {presetColors.map((c) => (
            <button
              key={c}
              className={`w-5 h-5 rounded border-2 ${color === c ? "border-blue-500" : "border-transparent"}`}
              style={{ backgroundColor: resolvePenColor(c, isDark) }}
              onClick={() => onColorChange(c)}
            />
          ))}
          <input
            type="color"
            value={resolvePenColor(color, isDark)}
            onChange={(e) => onColorChange(e.target.value)}
            className="w-5 h-5 rounded cursor-pointer"
          />
        </div>
      </div>

      <div className="flex items-center gap-1">
        <span className="text-xs text-[rgb(var(--muted))]">Size</span>
        <input
          type="range"
          min={2}
          max={32}
          value={size}
          onChange={(e) => onSizeChange(Number(e.target.value))}
          className="w-20"
        />
        <span className="text-xs text-[rgb(var(--muted))] w-6">{size}</span>
      </div>

      <div className="flex items-center gap-1">
        <button
          className={`px-2 py-1 text-xs rounded ${tool === "pen" ? "bg-blue-500 text-white" : "bg-[rgb(var(--border))]"}`}
          onClick={() => onToolChange("pen")}
        >
          Pen
        </button>
        <button
          className={`px-2 py-1 text-xs rounded ${tool === "text" ? "bg-blue-500 text-white" : "bg-[rgb(var(--border))]"}`}
          onClick={() => onToolChange("text")}
        >
          Text
        </button>
      </div>

      <button
        className="px-2 py-1 text-xs bg-[rgb(var(--border))] rounded hover:bg-[rgb(var(--muted))] transition-colors"
        onClick={handleUndo}
      >
        Undo
      </button>

      <button
        className={`px-2 py-1 text-xs rounded transition-colors ${
          showClearConfirm
            ? "bg-red-500 text-white"
            : "bg-[rgb(var(--border))] hover:bg-red-500 hover:text-white"
        }`}
        onClick={handleClear}
        onBlur={() => setShowClearConfirm(false)}
      >
        {showClearConfirm ? "Confirm Clear" : "Clear"}
      </button>
    </div>
  );
}
