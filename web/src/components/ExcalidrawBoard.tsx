import { useCallback, useRef } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawBoard as ExcalidrawBoardType } from "../ws/types";
import { sendWsMsg } from "../ws/manager";

interface Props {
  board: ExcalidrawBoardType;
  isHost: boolean;
}

export function ExcalidrawBoard({ board, isHost }: Props) {
  const versionRef = useRef<number>(board.sceneVersion);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  return (
    <div className="w-full h-full">
      <Excalidraw
        viewModeEnabled={!isHost}
        onChange={handleChange}
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        initialData={{
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          elements: board.elements as any,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          appState: board.appState as any,
        }}
      />
    </div>
  );
}
