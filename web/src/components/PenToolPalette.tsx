import { useState } from "react";
import { sendWsMsg } from "../ws/manager";

const PRESET_COLORS = [
  "#000000",
  "#ef4444",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#a855f7",
];

interface PenToolPaletteProps {
  boardId: string;
  onUndo?: () => void;
  onClear?: () => void;
}

type ToolMode = "pen" | "text";

export function PenToolPalette({ boardId, onUndo, onClear }: PenToolPaletteProps) {
  const [color, setColor] = useState("#000000");
  const [size, setSize] = useState(8);
  const [tool, setTool] = useState<ToolMode>("pen");
  const [showClearConfirm, setShowClearConfirm] = useState(false);

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
          {PRESET_COLORS.map((c) => (
            <button
              key={c}
              className={`w-5 h-5 rounded border-2 ${color === c ? "border-blue-500" : "border-transparent"}`}
              style={{ backgroundColor: c }}
              onClick={() => setColor(c)}
            />
          ))}
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
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
          onChange={(e) => setSize(Number(e.target.value))}
          className="w-20"
        />
        <span className="text-xs text-[rgb(var(--muted))] w-6">{size}</span>
      </div>

      <div className="flex items-center gap-1">
        <button
          className={`px-2 py-1 text-xs rounded ${tool === "pen" ? "bg-blue-500 text-white" : "bg-[rgb(var(--border))]"}`}
          onClick={() => setTool("pen")}
        >
          Pen
        </button>
        <button
          className={`px-2 py-1 text-xs rounded ${tool === "text" ? "bg-blue-500 text-white" : "bg-[rgb(var(--border))]"}`}
          onClick={() => setTool("text")}
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

      <input type="hidden" value={tool} />
      <input type="hidden" value={color} />
      <input type="hidden" value={size} />
    </div>
  );
}

export function getCurrentToolSettings() {
  return {
    color: "#000000",
    size: 8,
    tool: "pen" as ToolMode,
  };
}
