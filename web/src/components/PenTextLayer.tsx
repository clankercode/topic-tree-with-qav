import { useState, useRef, useEffect } from "react";
import type { PenText } from "../ws/types";
import type { ToolMode } from "./PenToolPalette";

const CANVAS_WIDTH = 4096;
const CANVAS_HEIGHT = 2304;

interface PenTextLayerProps {
  texts: PenText[];
  selectedTextId: string | null;
  onTextSelect?: (textId: string | null) => void;
  onTextCommit?: (
    textId: string,
    x: number,
    y: number,
    text: string,
    fontSize: number,
    color: string,
  ) => void;
  onTextDelete?: (textId: string) => void;
  isHost?: boolean;
  tool?: ToolMode;
}

interface TextEditState {
  id: string;
  x: number;
  y: number;
  text: string;
  fontSize: number;
  color: string;
}

export function PenTextLayer({
  texts,
  selectedTextId,
  onTextSelect,
  onTextCommit,
  onTextDelete,
  isHost = false,
  tool = "pen",
}: PenTextLayerProps) {
  const [editing, setEditing] = useState<TextEditState | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
    }
  }, [editing]);

  const handleCanvasClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isHost) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const scaleX = CANVAS_WIDTH / rect.width;
    const scaleY = CANVAS_HEIGHT / rect.height;
    const x = (e.clientX - rect.left) * scaleX;
    const y = (e.clientY - rect.top) * scaleY;

    setEditing({
      id: crypto.randomUUID(),
      x,
      y,
      text: "",
      fontSize: 24,
      color: "#000000",
    });
  };

  const handleTextClick = (e: React.MouseEvent, text: PenText) => {
    e.stopPropagation();
    if (!isHost) return;
    onTextSelect?.(text.id);
    setEditing({
      id: text.id,
      x: text.x,
      y: text.y,
      text: text.text,
      fontSize: text.fontSize,
      color: text.color,
    });
  };

  const handleCommit = () => {
    if (!editing) return;
    if (editing.text.trim()) {
      onTextCommit?.(
        editing.id,
        editing.x,
        editing.y,
        editing.text,
        editing.fontSize,
        editing.color,
      );
    }
    setEditing(null);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleCommit();
    }
    if (e.key === "Escape") {
      setEditing(null);
    }
    if (
      e.key === "Backspace" &&
      editing &&
      editing.text === "" &&
      selectedTextId
    ) {
      onTextDelete?.(selectedTextId);
      setEditing(null);
    }
  };

  return (
    <div
      className="absolute inset-0"
      style={{
        width: CANVAS_WIDTH,
        height: CANVAS_HEIGHT,
        transform: "scale(1)",
        transformOrigin: "top left",
        pointerEvents: tool === "text" ? "auto" : "none",
      }}
      onClick={handleCanvasClick}
    >
      {texts.map((text) => (
        <div
          key={text.id}
          className={`absolute cursor-pointer pointer-events-auto px-1 py-0.5 rounded select-none ${
            selectedTextId === text.id ? "ring-2 ring-blue-500" : ""
          }`}
          style={{
            left: (text.x / CANVAS_WIDTH) * 100 + "%",
            top: (text.y / CANVAS_HEIGHT) * 100 + "%",
            fontSize: text.fontSize + "px",
            color: text.color,
          }}
          onClick={(e) => handleTextClick(e, text)}
        >
          {text.text}
        </div>
      ))}

      {editing && (
        <input
          ref={inputRef}
          type="text"
          className="absolute pointer-events-auto px-1 py-0.5 rounded border outline-none"
          style={{
            left: (editing.x / CANVAS_WIDTH) * 100 + "%",
            top: (editing.y / CANVAS_HEIGHT) * 100 + "%",
            fontSize: editing.fontSize + "px",
            color: editing.color,
            minWidth: "100px",
            backgroundColor: "rgb(var(--pen-text-bg))",
            borderColor: "rgb(var(--pen-text-border))",
          }}
          value={editing.text}
          onChange={(e) => setEditing({ ...editing, text: e.target.value })}
          onBlur={handleCommit}
          onKeyDown={handleKeyDown}
        />
      )}
    </div>
  );
}
