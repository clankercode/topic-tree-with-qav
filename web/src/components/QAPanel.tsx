import { useSessionStore } from "../store";
import { QuestionComposer } from "./QuestionComposer";
import { QuestionList } from "./QuestionList";
import { SortToggle, type SortMode } from "./SortToggle";

interface QAPanelProps {
  sortMode: SortMode;
  onSortChange: (mode: SortMode) => void;
}

export function QAPanel({ sortMode, onSortChange }: QAPanelProps) {
  const questions = useSessionStore((s) => s.questions);
  const myVotes = useSessionStore((s) => s.myVotes);
  const me = useSessionStore((s) => s.me);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-between border-b border-[rgb(var(--border))] px-4 py-3">
        <h2 className="text-sm font-medium text-[rgb(var(--foreground))]">Q&amp;A</h2>
        <SortToggle sortMode={sortMode} onSortChange={onSortChange} />
      </div>

      {me?.role === "guest" && (
        <div className="border-b border-[rgb(var(--border))] p-3">
          <QuestionComposer />
        </div>
      )}

      <div className="flex-1 overflow-hidden">
        <QuestionList
          questions={questions}
          myVotes={myVotes}
          sortMode={sortMode}
        />
      </div>
    </div>
  );
}
