import { useEffect, useRef, useState } from "react";
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

  const [autoScrollLocked, setAutoScrollLocked] = useState(false);
  const [newQuestionsCount, setNewQuestionsCount] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const lastQuestionCountRef = useRef(questions.length);

  useEffect(() => {
    if (questions.length > lastQuestionCountRef.current && autoScrollLocked) {
      setNewQuestionsCount((c) => c + (questions.length - lastQuestionCountRef.current));
    }
    lastQuestionCountRef.current = questions.length;
  }, [questions.length, autoScrollLocked]);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;

    function handleScroll(this: HTMLDivElement) {
      const isAtBottom = this.scrollHeight - this.scrollTop - this.clientHeight < 50;
      setAutoScrollLocked(isAtBottom);
      if (isAtBottom) {
        setNewQuestionsCount(0);
      }
    }

    list.addEventListener("scroll", handleScroll);
    return () => list.removeEventListener("scroll", handleScroll);
  }, []);

  function handleJumpToNew() {
    setNewQuestionsCount(0);
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
    setAutoScrollLocked(true);
  }

  function handleJumpToBottom() {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
    setAutoScrollLocked(true);
  }

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

      <div className="flex-1 overflow-hidden" ref={listRef}>
        <QuestionList
          questions={questions}
          myVotes={myVotes}
          sortMode={sortMode}
          autoScrollLocked={autoScrollLocked}
          newQuestionsCount={newQuestionsCount}
          onJumpToNew={handleJumpToNew}
          onJumpToBottom={handleJumpToBottom}
        />
      </div>
    </div>
  );
}
