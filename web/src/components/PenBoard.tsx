import { useCallback, useRef, useState } from "react";
import { PenCanvas } from "./PenCanvas";
import { PenTextLayer } from "./PenTextLayer";
import { PenToolPalette } from "./PenToolPalette";
import { CursorLayer } from "./CursorLayer";
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
    (_bid: string, strokeId: string, _x: number, _y: number, _pressure: number) => {
      sendWsMsg({
        v: 1,
        type: "PenStrokeBegin",
        boardId,
        strokeId,
        color: "#000000",
        size: 8,
      });
    },
    [boardId],
  );

  const handleStrokeAppend = useCallback(
    (_bid: string, strokeId: string, x: number, y: number, pressure: number) => {
      const key = `${boardId}:${strokeId}`;
      const stroke = penInProgressStrokes.get(key);
      if (!stroke) return;
      const newPoints: [number, number, number][] = [[x, y, pressure]];
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
    (textId: string, x: number, y: number, text: string, fontSize: number, color: string) => {
      sendWsMsg({
        v: 1,
        type: "PenTextSet",
        boardId,
        textId,
        x,
        y,
        text,
        fontSize,
        color,
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
          <PenToolPalette boardId={boardId} />
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
        />
        <PenTextLayer
          texts={content.texts}
          selectedTextId={selectedTextId}
          onTextSelect={setSelectedTextId}
          onTextCommit={handleTextCommit}
          onTextDelete={handleTextDelete}
          isHost={isHost}
        />
        <CursorLayer
          boardId={boardId}
          containerRef={containerRef}
          onMouseMove={handleCursorMove}
          onMouseClick={handleClick}
        />
      </div>
    </div>
  );
}
