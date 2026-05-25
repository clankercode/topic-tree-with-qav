import { useCallback, useEffect, useRef } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawBoard as ExcalidrawBoardType } from "../ws/types";
import { sendWsMsg } from "../ws/manager";
import { CursorLayer } from "./CursorLayer";
import { ClickPingLayer } from "./ClickPingLayer";
import { useThemeStore } from "../store/theme";

interface Props {
  board: ExcalidrawBoardType;
  isHost: boolean;
}

function stripThemeFromAppState(appState: Record<string, unknown>) {
  const { theme: _theme, ...rest } = appState;
  return rest;
}

function mergeViewerTheme(
  appState: Record<string, unknown> | undefined,
  resolvedTheme: "light" | "dark",
) {
  return { ...(appState ?? {}), theme: resolvedTheme };
}

export function ExcalidrawBoard({ board, isHost }: Props) {
  const versionRef = useRef<number>(board.sceneVersion ?? 0);
  const boardIdRef = useRef<string>(board.id);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const apiRef = useRef<any>(null);
  const resolvedTheme = useThemeStore((s) => s.resolvedTheme);

  useEffect(() => {
    const incoming = board.sceneVersion ?? 0;
    const boardChanged = boardIdRef.current !== board.id;
    if (!boardChanged && incoming <= versionRef.current) return;
    boardIdRef.current = board.id;
    versionRef.current = incoming;
    if (apiRef.current) {
      apiRef.current.updateScene({
        elements: board.elements,
        appState: mergeViewerTheme(
          board.appState as Record<string, unknown> | undefined,
          resolvedTheme,
        ),
      });
    }
  }, [
    board.id,
    board.sceneVersion,
    board.elements,
    board.appState,
    resolvedTheme,
  ]);

  useEffect(() => {
    if (!apiRef.current) return;
    apiRef.current.updateScene({
      appState: { theme: resolvedTheme },
    });
  }, [resolvedTheme]);

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
          appState: stripThemeFromAppState(appState as Record<string, unknown>),
        });
      }, 150);
    },
    [isHost, board.id],
  );

  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

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
        excalidrawAPI={(api: any) => {
          apiRef.current = api;
        }}
        initialData={{
          elements: board.elements as never,
          appState: mergeViewerTheme(
            board.appState as Record<string, unknown> | undefined,
            resolvedTheme,
          ) as Record<string, unknown>,
        }}
      />
      <CursorLayer
        boardId={board.id}
        containerRef={containerRef}
        onMouseMove={handleCursorMove}
        onMouseClick={handleClick}
      />
      <ClickPingLayer boardId={board.id} containerRef={containerRef} />
    </div>
  );
}
