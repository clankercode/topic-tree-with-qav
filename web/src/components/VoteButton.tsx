import { ChevronUp } from "lucide-react";
import { sendWsMsg } from "../ws/manager";

interface VoteButtonProps {
  questionId: string;
  voteCount: number;
  hasVoted: boolean;
  disabled?: boolean;
}

export function VoteButton({ questionId, voteCount, hasVoted, disabled }: VoteButtonProps) {
  function handleVote() {
    if (disabled) return;
    sendWsMsg({
      v: 1,
      type: "VoteQuestion",
      questionId,
      vote: !hasVoted,
    });
  }

  return (
    <button
      onClick={handleVote}
      disabled={disabled}
      className={`flex flex-col items-center rounded border px-2 py-1 text-xs transition-colors ${
        hasVoted
          ? "border-[rgb(var(--primary))] bg-[rgb(var(--primary))] text-[rgb(var(--primary-fg))]"
          : "border-[rgb(var(--border))] bg-[rgb(var(--background))] text-[rgb(var(--muted))] hover:border-[rgb(var(--primary))] hover:text-[rgb(var(--primary))]"
      } ${disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer"}`}
      aria-label={hasVoted ? "Remove vote" : "Upvote"}
    >
      <ChevronUp size={14} />
      <span className="font-medium">{voteCount}</span>
    </button>
  );
}
