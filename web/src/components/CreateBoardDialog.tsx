import { useEffect, useId, useRef, useState } from "react";
import { sendWsMsg } from "../ws/manager";
import type { BoardKind } from "../ws/types";
import { useModalFocus } from "./useModalFocus";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function CreateBoardDialog({ open, onClose }: Props) {
  const [kind, setKind] = useState<BoardKind>("excalidraw");
  const [title, setTitle] = useState("");
  const dialogRef = useRef<HTMLDivElement>(null);
  const initialFocusRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();

  useModalFocus(open, dialogRef, onClose, initialFocusRef);

  useEffect(() => {
    if (open) {
      setKind("excalidraw");
      setTitle("");
    }
  }, [open]);

  if (!open) return null;

  function handleCreate() {
    sendWsMsg({
      v: 1,
      type: "CreateBoard",
      kind,
      title: title || undefined,
    });
    setTitle("");
    onClose();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        className="bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded-lg p-6 w-80 space-y-4"
      >
        <h2 id={titleId} className="text-lg font-semibold">
          Create Board
        </h2>
        <div className="space-y-2">
          <label className="block text-sm font-medium">Board Type</label>
          <div className="flex gap-2">
            <button
              type="button"
              ref={initialFocusRef}
              onClick={() => setKind("pen")}
              className={`flex-1 p-2 rounded border ${
                kind === "pen"
                  ? "border-[rgb(var(--accent))] bg-[rgb(var(--accent))/10]"
                  : "border-[rgb(var(--border))]"
              }`}
            >
              Pen
            </button>
            <button
              type="button"
              onClick={() => setKind("excalidraw")}
              className={`flex-1 p-2 rounded border ${
                kind === "excalidraw"
                  ? "border-[rgb(var(--accent))] bg-[rgb(var(--accent))/10]"
                  : "border-[rgb(var(--border))]"
              }`}
            >
              Excalidraw
            </button>
          </div>
        </div>
        <div className="space-y-2">
          <label className="block text-sm font-medium">Title (optional)</label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Untitled"
            className="w-full px-3 py-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--bg))] text-sm"
            maxLength={200}
          />
        </div>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 text-sm rounded border border-[rgb(var(--border))] hover:bg-[rgb(var(--muted))/10]"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleCreate}
            className="px-4 py-2 text-sm rounded bg-[rgb(var(--accent))] text-white hover:opacity-90"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
