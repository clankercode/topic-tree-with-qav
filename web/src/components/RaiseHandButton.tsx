import { useId, useRef, useState } from "react";
import { Hand, X } from "lucide-react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import {
  countTopicWords,
  isValidRaiseHandTopic,
  MAX_RAISE_HAND_TOPIC_LEN,
  MAX_RAISE_HAND_TOPIC_WORDS,
} from "../lib/validation";
import { useModalFocus } from "./useModalFocus";

export function RaiseHandButton() {
  const me = useSessionStore((s) => s.me);
  const hands = useSessionStore((s) => s.hands);
  const [open, setOpen] = useState(false);
  const [topic, setTopic] = useState("");
  const dialogRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  useModalFocus(open, dialogRef, () => setOpen(false), inputRef);

  if (me?.role !== "guest") return null;

  const myHand = hands.find((h) => h.guestId === me.guestId);
  const isRaised = !!myHand;

  const wordCount = countTopicWords(topic);
  const isValid = isValidRaiseHandTopic(topic);

  function handleOpen() {
    setTopic(myHand?.topic ?? "");
    setOpen(true);
  }

  function handleClose() {
    setOpen(false);
    setTopic("");
  }

  function handleRaise() {
    if (!isValid) return;
    sendWsMsg({
      v: 1,
      type: "RaiseHand",
      topic: topic.trim(),
    });
    handleClose();
  }

  function handleLower() {
    sendWsMsg({
      v: 1,
      type: "LowerHand",
    });
    handleClose();
  }

  return (
    <>
      <button
        onClick={handleOpen}
        className={`flex items-center gap-1.5 rounded border px-3 py-1.5 text-xs font-medium transition-colors ${
          isRaised
            ? "border-[rgb(var(--accent))] bg-[rgb(var(--accent))]/10 text-[rgb(var(--accent))]"
            : "border-[rgb(var(--border))] text-[rgb(var(--muted))] hover:border-[rgb(var(--accent))] hover:text-[rgb(var(--accent))]"
        }`}
        aria-label={isRaised ? "Lower hand" : "Raise hand"}
      >
        {isRaised ? <X size={14} /> : <Hand size={14} />}
        {isRaised ? "Lower hand" : "Raise hand"}
      </button>

      {open && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={handleClose}
        >
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            tabIndex={-1}
            onClick={(e) => e.stopPropagation()}
            className="bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded-lg w-80 space-y-4 p-6"
          >
            <div className="flex items-center justify-between">
              <h2 id={titleId} className="text-base font-semibold">
                {isRaised ? "Update your topic" : "Raise your hand"}
              </h2>
              <button
                onClick={handleClose}
                className="text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
              >
                <X size={16} />
              </button>
            </div>
            <p className="text-sm text-[rgb(var(--muted))]">
              In 10 words or fewer, please describe the topic.
            </p>
            <div className="space-y-2">
              <input
                ref={inputRef}
                type="text"
                value={topic}
                onChange={(e) => setTopic(e.target.value)}
                placeholder="e.g. Can you explain closures?"
                className="w-full px-3 py-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--bg))] text-sm"
                maxLength={MAX_RAISE_HAND_TOPIC_LEN}
              />
              <div className="flex justify-between text-xs text-[rgb(var(--muted))]">
                <span
                  className={
                    wordCount > MAX_RAISE_HAND_TOPIC_WORDS ? "text-red-500" : ""
                  }
                >
                  {wordCount}/{MAX_RAISE_HAND_TOPIC_WORDS} words
                </span>
                <span
                  className={
                    topic.length > MAX_RAISE_HAND_TOPIC_LEN ? "text-red-500" : ""
                  }
                >
                  {topic.length}/{MAX_RAISE_HAND_TOPIC_LEN} chars
                </span>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              {isRaised && (
                <button
                  type="button"
                  onClick={handleLower}
                  className="px-4 py-2 text-sm rounded border border-[rgb(var(--border))] text-[rgb(var(--muted))] hover:bg-[rgb(var(--muted))/10]"
                >
                  Lower hand
                </button>
              )}
              <button
                type="button"
                onClick={handleClose}
                className="px-4 py-2 text-sm rounded border border-[rgb(var(--border))] hover:bg-[rgb(var(--muted))/10]"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleRaise}
                disabled={!isValid}
                className="px-4 py-2 text-sm rounded bg-[rgb(var(--accent))] text-white hover:opacity-90 disabled:opacity-50"
              >
                {isRaised ? "Update" : "Raise hand"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
