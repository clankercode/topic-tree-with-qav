import { useState, useEffect, useRef } from "react";

interface Props {
  open: boolean;
  onClose: () => void;
  onAdd: (title: string) => void;
}

export function AddTopicModal({ open, onClose, onAdd }: Props) {
  const [title, setTitle] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setTitle("");
      setTimeout(() => inputRef.current?.focus(), 0);
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded-lg p-6 w-80 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold">Add Topic</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <input
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
              className="px-4 py-2 text-sm rounded border border-[rgb(var(--border))] hover:bg-[rgb(var(--muted))/10]"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!title.trim()}
              className="px-4 py-2 text-sm rounded bg-[rgb(var(--primary))] text-[rgb(var(--primary-fg))] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Add
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}