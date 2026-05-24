import { useCallback, useRef, useState } from "react";
import { PenCanvas } from "./PenCanvas";
import { PenTextLayer } from "./PenTextLayer";
import { PenToolPalette, type ToolMode } from "./PenToolPalette";
import { CursorLayer } from "./CursorLayer";
import { ClickPingLayer } from "./ClickPingLayer";
import { sendWsMsg } from "../ws/manager";
import { useSessionStore } from "../store";
import type { PenBoardContent } from "../ws/types";

interface PenBoardProps {
  boardId: string;
  content: PenBoardContent;
  isHost?: boolean;
}

export function PenBoard({ boardId, content, isHost = false }: PenBoardProps) {
  const [selectedTextId, setSelectedTextId] = useState<string | null>(null);
  const [color, setColor] = useState("#000000");
  const [size, setSize] = useState(8);
  const [tool, setTool] = useState<ToolMode>("pen");
  const penInProgressStrokes = useSessionStore((s) => s.penInProgressStrokes);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleCursorMove = useCallback(
    (x: number, y: number) => {
      sendWsMsg({ v: 1, type: "Cursor", boardId, x, y });
    },
    [boardId],
  );

  const handleClick = useCallback(
    (x: number, y: number) => {
      sendWsMsg({ v: 1, type: "Click", boardId, x, y });
    },
    [boardId],
  );

  const getAllInProgressForBoard = useCallback(() => {
    const result = new Map<string, { color: string; size: number; points: [number, number, number][] }>();
    penInProgressStrokes.forEach((v, k) => {
      if (k.startsWith(boardId + ":")) {
        const strokeId = k.split(":")[1];
        result.set(strokeId, v);
      }
    });
    return result;
  }, [boardId, penInProgressStrokes]);

  const handleStrokeBegin = useCallback(
    (_bid: string, strokeId: string, x: number, y: number, pressure: number) => {
      const key = `${boardId}:${strokeId}`;
      const newInProgress = new Map(penInProgressStrokes);
      newInProgress.set(key, { color, size, points: [[x, y, pressure] as [number, number, number]] });
      useSessionStore.setState({ penInProgressStrokes: newInProgress });
      sendWsMsg({
        v: 1,
        type: "PenStrokeBegin",
        boardId,
        strokeId,
        color,
        size,
      });
    },
    [boardId, penInProgressStrokes, color, size],
  );

  const handleStrokeAppend = useCallback(
    (_bid: string, strokeId: string, x: number, y: number, pressure: number) => {
      const key = `${boardId}:${strokeId}`;
      const stroke = penInProgressStrokes.get(key);
      if (!stroke) return;
      const newPoints: [number, number, number][] = [[x, y, pressure]];
      const newInProgress = new Map(penInProgressStrokes);
      newInProgress.set(key, { ...stroke, points: [...stroke.points, ...newPoints] });
      useSessionStore.setState({ penInProgressStrokes: newInProgress });
      sendWsMsg({
        v: 1,
        type: "PenStrokeAppend",
        boardId,
        strokeId,
        points: newPoints,
      });
    },
    [boardId, penInProgressStrokes],
  );

  const handleStrokeEnd = useCallback(
    (_bid: string, strokeId: string) => {
      sendWsMsg({
        v: 1,
        type: "PenStrokeEnd",
        boardId,
        strokeId,
      });
    },
    [boardId],
  );

  const handleTextCommit = useCallback(
    (textId: string, x: number, y: number, text: string, fontSize: number, textColor: string) => {
      sendWsMsg({
        v: 1,
        type: "PenTextSet",
        boardId,
        textId,
        x,
        y,
        text,
        fontSize,
        color: textColor,
      });
    },
    [boardId],
  );

  const handleTextDelete = useCallback(
    (textId: string) => {
      sendWsMsg({
        v: 1,
        type: "PenTextDelete",
        boardId,
        textId,
      });
    },
    [boardId],
  );

  const inProgressStrokes = getAllInProgressForBoard();

  return (
    <div className="flex flex-col h-full">
      {isHost && (
        <div className="mb-2">
          <PenToolPalette
            boardId={boardId}
            color={color}
            size={size}
            tool={tool}
            onColorChange={setColor}
            onSizeChange={setSize}
            onToolChange={setTool}
          />
        </div>
      )}
      <div className="relative flex-1 min-h-0 border border-[rgb(var(--border))] rounded overflow-hidden" ref={containerRef}>
        <PenCanvas
          boardId={boardId}
          strokes={content.strokes}
          inProgressStrokes={inProgressStrokes}
          onStrokeBegin={handleStrokeBegin}
          onStrokeAppend={handleStrokeAppend}
          onStrokeEnd={handleStrokeEnd}
          isHost={isHost}
          tool={tool}
        />
        <PenTextLayer
          texts={content.texts}
          selectedTextId={selectedTextId}
          onTextSelect={setSelectedTextId}
          onTextCommit={handleTextCommit}
          onTextDelete={handleTextDelete}
          isHost={isHost}
          tool={tool}
        />
        <CursorLayer
          boardId={boardId}
          containerRef={containerRef}
          onMouseMove={handleCursorMove}
          onMouseClick={handleClick}
        />
        <ClickPingLayer
          boardId={boardId}
          containerRef={containerRef}
        />
      </div>
    </div>
  );
}