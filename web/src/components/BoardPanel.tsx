import { useState } from "react";
import { Plus } from "lucide-react";
import { useSessionStore } from "../store";
import { useFollowHostStore } from "../store/followHost";
import { sendWsMsg } from "../ws/manager";
import { BoardTabs } from "./BoardTabs";
import { CreateBoardDialog } from "./CreateBoardDialog";
import { ExcalidrawBoard } from "./ExcalidrawBoard";
import { PenBoard } from "./PenBoard";
import type { ExcalidrawBoard as ExcalidrawBoardType } from "../ws/types";

export function BoardPanel() {
  const boards = useSessionStore((s) => s.boards);
  const focusedBoardId = useSessionStore((s) => s.focusedBoardId);
  const me = useSessionStore((s) => s.me);
  const penBoards = useSessionStore((s) => s.penBoards);
  const followingHost = useFollowHostStore((s) => s.followingHost);
  const setFollowingHost = useFollowHostStore((s) => s.setFollowingHost);
  const [showCreate, setShowCreate] = useState(false);
  const isHost = me?.role === "host";

  const focusedBoard = boards.find((b) => b.id === focusedBoardId) ?? boards[0];

  function handleSelectBoard(boardId: string) {
    if (!isHost) return;
    sendWsMsg({ v: 1, type: "SetFocusedBoard", boardId });
  }

  if (boards.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 p-8 text-center">
        <p className="text-[rgb(var(--muted))]">No boards yet.</p>
        {isHost && (
          <button
            type="button"
            onClick={() => setShowCreate(true)}
            className="flex items-center gap-2 px-4 py-2 rounded bg-[rgb(var(--accent))] text-white hover:opacity-90"
          >
            <Plus className="w-4 h-4" />
            Create Board
          </button>
        )}
        <CreateBoardDialog open={showCreate} onClose={() => setShowCreate(false)} />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 border-b border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-2">
        <BoardTabs
          boards={boards}
          focusedBoardId={focusedBoard?.id ?? null}
          isHost={isHost}
          onSelectBoard={handleSelectBoard}
        />
        {isHost && (
          <button
            type="button"
            onClick={() => setShowCreate(true)}
            className="p-1.5 rounded hover:bg-[rgb(var(--muted))/10]"
            title="Create Board"
          >
            <Plus className="w-4 h-4" />
          </button>
        )}
        {!isHost && (
          <label className="ml-auto flex items-center gap-2 text-xs text-[rgb(var(--muted))] cursor-pointer">
            <input
              type="checkbox"
              checked={followingHost}
              onChange={(e) => setFollowingHost(e.target.checked)}
              className="w-3.5 h-3.5 rounded border-[rgb(var(--border))] accent-[rgb(var(--accent))]"
            />
            Follow host
          </label>
        )}
      </div>
      <div className="flex-1 overflow-hidden flex items-center justify-center">
        {focusedBoard?.kind === "excalidraw" ? (
          <div className="w-full h-full">
            <ExcalidrawBoard board={focusedBoard as ExcalidrawBoardType} isHost={isHost} />
          </div>
        ) : focusedBoard?.kind === "pen" ? (
          <div className="w-full max-w-full" style={{ aspectRatio: "16/9" }}>
            <PenBoard boardId={focusedBoard.id} content={penBoards.get(focusedBoard.id) ?? { strokes: [], texts: [] }} isHost={isHost} />
          </div>
        ) : (
          <div className="flex items-center justify-center h-full text-[rgb(var(--muted))]">
            Select a board
          </div>
        )}
      </div>
      <CreateBoardDialog open={showCreate} onClose={() => setShowCreate(false)} />
    </div>
  );
}
