import { useCallback, useRef, useEffect } from "react";
import { Excalidraw, type ExcalidrawImperativeAPI } from "@excalidraw/excalidraw";
import type { ExcalidrawBoard as ExcalidrawBoardType } from "../ws/types";
import { sendWsMsg } from "../ws/manager";
import { CursorLayer } from "./CursorLayer";
import { ClickPingLayer } from "./ClickPingLayer";
import { useThemeStore } from "../store/theme";

interface Props {
  board: ExcalidrawBoardType;
  isHost: boolean;
}

export function ExcalidrawBoard({ board, isHost }: Props) {
  const versionRef = useRef<number>(board.sceneVersion ?? 0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const excalidrawRef = useRef<ExcalidrawImperativeAPI | null>(null);
  const resolvedTheme = useThemeStore((s) => s.resolvedTheme);

  useEffect(() => {
    if (board.sceneVersion > versionRef.current && excalidrawRef.current) {
      excalidrawRef.current.updateScene({
        elements: board.elements as any,
        appState: board.appState as any,
      });
      versionRef.current = board.sceneVersion;
    }
  }, [board.sceneVersion, board.elements, board.appState]);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const handleChange = useCallback(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (elements: readonly any[], appState: any) => {
      if (!isHost) return;
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        const newVersion = versionRef.current + 1;
        versionRef.current = newVersion;
        sendWsMsg({
          v: 1,
          type: "ExcalidrawUpdate",
          boardId: board.id,
          sceneVersion: newVersion,
          elements: elements as unknown[],
          appState,
        });
      }, 150);
    },
    [isHost, board.id],
  );

  const handleCursorMove = useCallback(
    (x: number, y: number) => {
      sendWsMsg({ v: 1, type: "Cursor", boardId: board.id, x, y });
    },
    [board.id],
  );

  const handleClick = useCallback(
    (x: number, y: number) => {
      sendWsMsg({ v: 1, type: "Click", boardId: board.id, x, y });
    },
    [board.id],
  );

  return (
    <div className="w-full h-full relative" ref={containerRef}>
      <Excalidraw
        viewModeEnabled={!isHost}
        onChange={handleChange}
        theme={resolvedTheme}
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        initialData={{
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          elements: board.elements as any,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          appState: { ...(board.appState as any), theme: resolvedTheme },
        }}
        excalidrawRef={excalidrawRef}
      />
      <CursorLayer
        boardId={board.id}
        containerRef={containerRef}
        onMouseMove={handleCursorMove}
        onMouseClick={handleClick}
      />
      <ClickPingLayer
        boardId={board.id}
        containerRef={containerRef}
      />
    </div>
  );
}
