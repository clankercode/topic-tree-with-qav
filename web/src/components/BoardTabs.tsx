import { useState } from "react";
import { MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { sendWsMsg } from "../ws/manager";
import type { FatBoard } from "../ws/types";

interface Props {
  boards: FatBoard[];
  focusedBoardId: string | null;
  isHost: boolean;
  onSelectBoard: (boardId: string) => void;
}

export function BoardTabs({ boards, focusedBoardId, isHost, onSelectBoard }: Props) {
  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  if (boards.length === 0) return null;

  function handleRename(board: FatBoard) {
    setRenaming(board.id);
    setRenameValue(board.title);
    setMenuOpen(null);
  }

  function handleRenameSubmit(boardId: string) {
    if (renameValue.trim()) {
      sendWsMsg({
        v: 1,
        type: "RenameBoard",
        boardId,
        title: renameValue.trim(),
      });
    }
    setRenaming(null);
  }

  function handleDelete(boardId: string) {
    if (confirm("Delete this board?")) {
      sendWsMsg({ v: 1, type: "DeleteBoard", boardId });
    }
    setMenuOpen(null);
  }

  return (
    <div className="flex items-center gap-1 border-b border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-2 overflow-x-auto">
      {boards.map((board) => (
        <div key={board.id} className="relative">
          <button
            type="button"
            onClick={() => onSelectBoard(board.id)}
            className={`flex items-center gap-1.5 px-3 py-2 text-sm whitespace-nowrap border-b-2 transition-colors ${
              focusedBoardId === board.id
                ? "border-[rgb(var(--accent))] text-[rgb(var(--accent))]"
                : "border-transparent hover:bg-[rgb(var(--muted))/10]"
            }`}
          >
            <KindIcon kind={board.kind} />
            {renaming === board.id ? (
              <input
                type="text"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onBlur={() => handleRenameSubmit(board.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleRenameSubmit(board.id);
                  if (e.key === "Escape") setRenaming(null);
                }}
                className="w-24 px-1 py-0.5 text-sm bg-[rgb(var(--bg))] border border-[rgb(var(--accent))] rounded"
                autoFocus
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span className="max-w-32 truncate">{board.title}</span>
            )}
            {isHost && (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenuOpen(menuOpen === board.id ? null : board.id);
                }}
                className="p-0.5 rounded hover:bg-[rgb(var(--muted))/20]"
              >
                <MoreHorizontal className="w-4 h-4" />
              </button>
            )}
          </button>
          {isHost && menuOpen === board.id && (
            <div className="absolute top-full left-0 mt-1 bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded shadow-lg z-10 min-w-32">
              <button
                type="button"
                onClick={() => handleRename(board)}
                className="flex items-center gap-2 w-full px-3 py-2 text-sm text-left hover:bg-[rgb(var(--muted))/10]"
              >
                <Pencil className="w-4 h-4" />
                Rename
              </button>
              <button
                type="button"
                onClick={() => handleDelete(board.id)}
                className="flex items-center gap-2 w-full px-3 py-2 text-sm text-left hover:bg-[rgb(var(--muted))/10] text-red-500"
              >
                <Trash2 className="w-4 h-4" />
                Delete
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function KindIcon({ kind }: { kind: string }) {
  if (kind === "pen") {
    return <Pencil className="w-4 h-4" />;
  }
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <line x1="9" y1="9" x2="15" y2="15" />
      <line x1="15" y1="9" x2="9" y2="15" />
    </svg>
  );
}
