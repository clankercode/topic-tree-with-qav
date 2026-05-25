import { useCallback, useEffect, useRef, useState } from "react";
import { Send } from "lucide-react";
import { registerPendingSubmit, sendWsMsg } from "../ws/manager";
import { useSessionStore } from "../store";
import { useToastStore } from "../store/toast";

interface QuestionComposerProps {
  onSubmitted?: () => void;
}

const SUBMIT_TIMEOUT_MS = 5000;

/// G.5: preserve draft text until the matching `Ack` lands. On Error
/// with code `rate_limit` or `muted`, restore the input and surface
/// a toast so the user can edit + retry instead of retyping.
export function QuestionComposer({ onSubmitted }: QuestionComposerProps) {
  const [text, setText] = useState("");
  const [anonymous, setAnonymous] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const me = useSessionStore((s) => s.me);
  const connectionStatus = useSessionStore((s) => s.connectionStatus);
  const addToast = useToastStore((s) => s.addToast);
  // Cleanup callbacks for in-flight submissions: registers run during
  // submit; if the component unmounts before the ack/error, we let
  // resolution still happen (no DOM access), so cleanup is best-effort.
  const cleanupRef = useRef<(() => void) | null>(null);
  const timeoutRef = useRef<number | null>(null);
  const restorePendingRef = useRef<(() => void) | null>(null);
  const clearPendingSubmission = useCallback(() => {
    cleanupRef.current?.();
    cleanupRef.current = null;
    if (timeoutRef.current !== null) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    restorePendingRef.current = null;
  }, []);

  useEffect(
    () => () => {
      clearPendingSubmission();
    },
    [clearPendingSubmission],
  );

  useEffect(() => {
    if (connectionStatus === "connected") return;
    const restore = restorePendingRef.current;
    if (!restore) return;
    clearPendingSubmission();
    restore();
  }, [clearPendingSubmission, connectionStatus]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = text.trim();
    if (!trimmed || isSubmitting || !me) return;

    const refId = crypto.randomUUID();
    const submittedText = trimmed;
    const submittedAnonymous = anonymous;

    setIsSubmitting(true);
    // Clear immediately so the user sees a responsive UI. If the
    // server rejects, the rollback handler below restores both
    // fields verbatim.
    setText("");

    clearPendingSubmission();
    restorePendingRef.current = () => {
      setText(submittedText);
      setAnonymous(submittedAnonymous);
      setIsSubmitting(false);
      addToast("Submission timed out — please retry.", "error");
    };
    timeoutRef.current = window.setTimeout(() => {
      const restore = restorePendingRef.current;
      if (!restore) return;
      clearPendingSubmission();
      restore();
    }, SUBMIT_TIMEOUT_MS);
    cleanupRef.current = registerPendingSubmit(refId, (outcome) => {
      clearPendingSubmission();
      setIsSubmitting(false);
      if (outcome.kind === "ack") {
        onSubmitted?.();
        return;
      }
      if (outcome.code === "rate_limit" || outcome.code === "muted") {
        setText(submittedText);
        setAnonymous(submittedAnonymous);
        // The reducer already toasts for rate_limit / muted, so we
        // just need to make sure the user knows their draft is back.
      } else {
        // Unknown error path: surface a generic toast and keep the
        // draft so nothing is lost.
        setText(submittedText);
        setAnonymous(submittedAnonymous);
        addToast(`Could not submit question: ${outcome.message}`, "error");
      }
    });

    sendWsMsg({
      v: 1,
      type: "SubmitQuestion",
      id: refId,
      text: submittedText,
      anonymous: submittedAnonymous,
    });
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] p-3"
    >
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
