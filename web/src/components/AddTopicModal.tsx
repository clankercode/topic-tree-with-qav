import { useEffect, useId, useRef, useState } from "react";
import { useModalFocus } from "./useModalFocus";

interface Props {
  open: boolean;
  onClose: () => void;
  onAdd: (title: string) => void;
}

export function AddTopicModal({ open, onClose, onAdd }: Props) {
  const [title, setTitle] = useState("");
  const dialogRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const inputId = useId();

  useModalFocus(open, dialogRef, onClose, inputRef);

  useEffect(() => {
    if (open) {
      setTitle("");
    }
  }, [open]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    onAdd(title.trim());
    onClose();
  }

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      onClick={onClose}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="w-full max-w-sm space-y-4 rounded-lg border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id={titleId} className="text-lg font-semibold">
          Add Topic
        </h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <label htmlFor={inputId} className="sr-only">
            Topic title
          </label>
          <input
            id={inputId}
            ref={inputRef}
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Topic title"
            className="w-full px-3 py-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] text-sm placeholder:text-[rgb(var(--muted))] focus:border-[rgb(var(--primary))] focus:outline-none focus:ring-1 focus:ring-[rgb(var(--primary))]"
            maxLength={200}
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded border border-[rgb(var(--border))] px-4 py-2 text-sm hover:bg-[rgb(var(--muted)/0.12)]"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!title.trim()}
              className="rounded bg-[rgb(var(--primary))] px-4 py-2 text-sm text-[rgb(var(--primary-fg))] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Add
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
