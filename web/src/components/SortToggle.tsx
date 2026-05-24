import { ArrowUpDown } from "lucide-react";

export type SortMode = "chronological" | "votes";

interface SortToggleProps {
  sortMode: SortMode;
  onSortChange: (mode: SortMode) => void;
}

export function SortToggle({ sortMode, onSortChange }: SortToggleProps) {
  return (
    <div className="flex items-center gap-2">
      <ArrowUpDown size={14} className="text-[rgb(var(--muted))]" />
      <select
        value={sortMode}
        onChange={(e) => onSortChange(e.target.value as SortMode)}
        className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-2 py-1 text-xs text-[rgb(var(--foreground))]"
      >
        <option value="chronological">Newest first</option>
        <option value="votes">By votes</option>
      </select>
    </div>
  );
}
