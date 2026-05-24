import { Hand, User, Check } from "lucide-react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";

export function HandsQueue() {
  const me = useSessionStore((s) => s.me);
  const hands = useSessionStore((s) => s.hands);

  if (me?.role !== "host") return null;

  function handleCallOn(guestId: string) {
    sendWsMsg({
      v: 1,
      type: "CallOnHand",
      guestId,
    });
  }

  function handleDismiss(guestId: string) {
    sendWsMsg({
      v: 1,
      type: "DismissHand",
      guestId,
    });
  }

  const sortedHands = [...hands].sort((a, b) => a.raisedAt - b.raisedAt);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-sm font-medium text-[rgb(var(--foreground))]">
        <Hand size={16} />
        Raised Hands
        {hands.length > 0 && (
          <span className="ml-1 rounded bg-[rgb(var(--accent))]/10 px-1.5 py-0.5 text-xs text-[rgb(var(--accent))]">
            {hands.length}
          </span>
        )}
      </div>

      {sortedHands.length === 0 ? (
        <p className="py-4 text-center text-xs text-[rgb(var(--muted))]">No raised hands.</p>
      ) : (
        <div className="flex flex-col gap-2">
          {sortedHands.map((hand) => (
            <div
              key={hand.guestId}
              className="flex items-start gap-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-2"
            >
              <div className="flex flex-1 flex-col gap-0.5">
                <div className="flex items-center gap-1 text-xs text-[rgb(var(--muted))]">
                  <User size={10} />
                  {hand.displayName}
                </div>
                <p className="text-sm">{hand.topic}</p>
              </div>
              <div className="flex gap-1">
                <button
                  onClick={() => handleCallOn(hand.guestId)}
                  className="rounded p-1 text-[rgb(var(--success))] hover:bg-[rgb(var(--success))]/10"
                  aria-label="Call on"
                  title="Call on"
                >
                  <Check size={14} />
                </button>
                <button
                  onClick={() => handleDismiss(hand.guestId)}
                  className="rounded p-1 text-[rgb(var(--muted))] hover:bg-red-500/10 hover:text-red-500"
                  aria-label="Dismiss"
                  title="Dismiss"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M18 6L6 18M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
