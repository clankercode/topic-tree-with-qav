import { useState } from "react";
import { Send } from "lucide-react";
import { sendWsMsg } from "../ws/manager";
import { useSessionStore } from "../store";

interface QuestionComposerProps {
  onSubmitted?: () => void;
}

export function QuestionComposer({ onSubmitted }: QuestionComposerProps) {
  const [text, setText] = useState("");
  const [anonymous, setAnonymous] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const me = useSessionStore((s) => s.me);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!text.trim() || isSubmitting || !me) return;

    setIsSubmitting(true);
    sendWsMsg({
      v: 1,
      type: "SubmitQuestion",
      text: text.trim(),
      anonymous,
    });
    setText("");
    setIsSubmitting(false);
    onSubmitted?.();
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] p-3">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Ask a question..."
        maxLength={500}
        rows={2}
        className="w-full resize-none rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-3 py-2 text-sm text-[rgb(var(--foreground))] placeholder:text-[rgb(var(--muted))] focus:border-[rgb(var(--primary))] focus:outline-none focus:ring-1 focus:ring-[rgb(var(--primary))]"
      />
      <div className="flex items-center justify-between">
        <label className="flex items-center gap-2 text-xs text-[rgb(var(--muted))]">
          <input
            type="checkbox"
            checked={anonymous}
            onChange={(e) => setAnonymous(e.target.checked)}
            className="rounded border-[rgb(var(--border))]"
          />
          Ask anonymously
        </label>
        <button
          type="submit"
          disabled={!text.trim() || isSubmitting}
          className="flex items-center gap-1 rounded bg-[rgb(var(--primary))] px-3 py-1.5 text-xs font-medium text-[rgb(var(--primary-fg))] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Send size={12} />
          Submit
        </button>
      </div>
    </form>
  );
}
