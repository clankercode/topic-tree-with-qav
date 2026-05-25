import { useState } from "react";
import { Hand, X } from "lucide-react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";

export function RaiseHandButton() {
  const me = useSessionStore((s) => s.me);
  const hands = useSessionStore((s) => s.hands);
  const [open, setOpen] = useState(false);
  const [topic, setTopic] = useState("");

  if (me?.role !== "guest") return null;

  const myHand = hands.find((h) => h.guestId === me.guestId);
  const isRaised = !!myHand;

  const wordCount = topic.trim().split(/\s+/).filter(Boolean).length;
  const isValid =
    topic.trim().length > 0 && topic.length <= 80 && wordCount <= 10;

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
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded-lg w-80 space-y-4 p-6">
            <div className="flex items-center justify-between">
              <h2 className="text-base font-semibold">
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
                type="text"
                value={topic}
                onChange={(e) => setTopic(e.target.value)}
                placeholder="e.g. Can you explain closures?"
                className="w-full px-3 py-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--bg))] text-sm"
                maxLength={80}
                autoFocus
              />
              <div className="flex justify-between text-xs text-[rgb(var(--muted))]">
                <span className={wordCount > 10 ? "text-red-500" : ""}>
                  {wordCount}/10 words
                </span>
                <span className={topic.length > 80 ? "text-red-500" : ""}>
                  {topic.length}/80 chars
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
