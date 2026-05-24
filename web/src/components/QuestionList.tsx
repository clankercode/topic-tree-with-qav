import { Check, Trash2, Plus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Question } from "../ws/types";
import { VoteButton } from "./VoteButton";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";

interface QuestionItemProps {
  question: Question;
  hasVoted: boolean;
}

export function QuestionItem({ question, hasVoted }: QuestionItemProps) {
  const me = useSessionStore((s) => s.me);
  const isHost = me?.role === "host";

  function handleMarkAnswered() {
    if (!isHost) return;
    sendWsMsg({
      v: 1,
      type: "MarkQuestionAnswered",
      questionId: question.id,
      answered: !question.answered,
    });
  }

  function handleDelete() {
    if (!isHost) return;
    sendWsMsg({
      v: 1,
      type: "DeleteQuestion",
      questionId: question.id,
    });
  }

  function handlePromoteToTopic() {
    if (!isHost) return;
    sendWsMsg({
      v: 1,
      type: "PromoteQuestionToTopic",
      questionId: question.id,
    });
  }

  const displayName = question.anonymous ? "Anonymous" : question.authorName;

  return (
    <div
      className={`flex gap-3 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] p-3 transition-opacity ${
        question.answered ? "opacity-60" : ""
      }`}
    >
      {me?.role !== "host" && (
        <VoteButton
          questionId={question.id}
          voteCount={question.voteCount}
          hasVoted={hasVoted}
        />
      )}
      <div className="flex flex-1 flex-col gap-1">
        <p className={`text-sm ${question.answered ? "text-[rgb(var(--muted))] line-through" : "text-[rgb(var(--foreground))]"}`}>
          {question.text}
        </p>
        <div className="flex items-center gap-2 text-xs text-[rgb(var(--muted))]">
          <span>{displayName}</span>
          {question.answered && (
            <span className="flex items-center gap-1 text-[rgb(var(--success))]">
              <Check size={10} />
              Answered
            </span>
          )}
        </div>
      </div>
      {isHost && (
        <div className="flex items-center gap-1">
          <button
            onClick={handlePromoteToTopic}
            className="rounded p-1 text-[rgb(var(--accent))] hover:bg-[rgb(var(--accent))]/10"
            aria-label="Promote to topic"
            title="Promote to topic"
          >
            <Plus size={14} />
          </button>
          <button
            onClick={handleMarkAnswered}
            className={`rounded p-1 text-xs transition-colors ${
              question.answered
                ? "text-[rgb(var(--success))] hover:bg-[rgb(var(--success))]/10"
                : "text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
            }`}
            aria-label={question.answered ? "Mark as unanswered" : "Mark as answered"}
          >
            <Check size={14} />
          </button>
          <button
            onClick={handleDelete}
            className="rounded p-1 text-[rgb(var(--muted))] hover:bg-red-500/10 hover:text-red-500"
            aria-label="Delete question"
          >
            <Trash2 size={14} />
          </button>
        </div>
      )}
    </div>
  );
}

interface QuestionListProps {
  questions: Question[];
  myVotes: Set<string>;
  sortMode: "chronological" | "votes";
}

export function QuestionList({
  questions,
  myVotes,
  sortMode,
}: QuestionListProps) {
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [newCount, setNewCount] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastAtBottomRef = useRef(true);
  const lastCountRef = useRef(questions.length);

  useEffect(() => {
    const newQ = questions.length - lastCountRef.current;
    if (newQ > 0 && lastAtBottomRef.current) {
      setNewCount((c) => c + newQ);
    }
    lastCountRef.current = questions.length;
  }, [questions.length]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    function handleScroll() {
      if (!el) return;
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
      if (atBottom !== lastAtBottomRef.current) {
        lastAtBottomRef.current = atBottom;
        setIsAtBottom(atBottom);
        if (atBottom) {
          setNewCount(0);
        }
      }
    }

    el.addEventListener("scroll", handleScroll);
    return () => el.removeEventListener("scroll", handleScroll);
  }, []);

  function handleJumpToNew() {
    setNewCount(0);
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }

  const sorted = [...questions].sort((a, b) => {
    if (a.answered !== b.answered) {
      return a.answered ? 1 : -1;
    }
    if (sortMode === "votes") {
      if (b.voteCount !== a.voteCount) {
        return b.voteCount - a.voteCount;
      }
    }
    return a.createdAt - b.createdAt;
  });

  return (
    <div className="relative flex flex-1 flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto" ref={scrollRef} id="qa-list">
        {sorted.length === 0 ? (
          <p className="py-8 text-center text-sm text-[rgb(var(--muted))]">No questions yet. Be the first to ask!</p>
        ) : (
          <div className="flex flex-col gap-2 p-1">
            {sorted.map((q) => (
              <QuestionItem key={q.id} question={q} hasVoted={myVotes.has(q.id)} />
            ))}
          </div>
        )}
      </div>

      {newCount > 0 && isAtBottom && (
        <button
          onClick={handleJumpToNew}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 animate-bounce rounded-full bg-[rgb(var(--primary))] px-4 py-2 text-xs font-medium text-[rgb(var(--primary-fg))] shadow-lg"
        >
          {newCount} new question{newCount > 1 ? "s" : ""}
        </button>
      )}

      {!isAtBottom && (
        <button
          onClick={handleJumpToNew}
          className="absolute bottom-4 right-4 rounded-full bg-[rgb(var(--muted))] p-2 text-[rgb(var(--background))] shadow-lg hover:bg-[rgb(var(--foreground))]"
          aria-label="Jump to bottom"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 5v14M19 12l-7 7-7-7" />
          </svg>
        </button>
      )}
    </div>
  );
}
