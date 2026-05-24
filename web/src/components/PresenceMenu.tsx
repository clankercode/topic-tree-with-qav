import { useState } from "react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import { MoreHorizontal, MicOff, Mic, UserX } from "lucide-react";

export function PresenceMenu() {
  const me = useSessionStore((s) => s.me);
  const presence = useSessionStore((s) => s.presence);
  const [openGuestId, setOpenGuestId] = useState<string | null>(null);

  if (me?.role !== "host") return null;

  const otherGuests = presence.filter((g) => g.guestId !== me.guestId);

  if (otherGuests.length === 0) return null;

  return (
    <div className="flex items-center gap-2">
      {otherGuests.map((guest) => (
        <div key={guest.guestId} className="relative">
          <button
            onClick={() => setOpenGuestId(openGuestId === guest.guestId ? null : guest.guestId)}
            className="flex items-center gap-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-3 py-1.5 text-sm hover:bg-[rgb(var(--border))]"
          >
            <span className={guest.muted ? "text-[rgb(var(--muted))]" : ""}>
              {guest.displayName}
            </span>
            {guest.muted && <MicOff className="h-3 w-3 text-[rgb(var(--muted))]" />}
            <MoreHorizontal className="h-3 w-3" />
          </button>
          {openGuestId === guest.guestId && (
            <div className="absolute right-0 top-full mt-1 z-50 min-w-[140px] rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] shadow-lg">
              <button
                onClick={() => {
                  sendWsMsg({
                    v: 1,
                    id: crypto.randomUUID(),
                    type: "MuteGuest",
                    guestId: guest.guestId,
                    muted: !guest.muted,
                  });
                  setOpenGuestId(null);
                }}
                className="flex w-full items-center gap-2 px-3 py-2 text-sm hover:bg-[rgb(var(--border))]"
              >
                {guest.muted ? (
                  <>
                    <Mic className="h-4 w-4" />
                    Unmute
                  </>
                ) : (
                  <>
                    <MicOff className="h-4 w-4" />
                    Mute
                  </>
                )}
              </button>
              <button
                onClick={() => {
                  sendWsMsg({
                    v: 1,
                    id: crypto.randomUUID(),
                    type: "KickGuest",
                    guestId: guest.guestId,
                  });
                  setOpenGuestId(null);
                }}
                className="flex w-full items-center gap-2 px-3 py-2 text-sm text-red-400 hover:bg-[rgb(var(--border))]"
              >
                <UserX className="h-4 w-4" />
                Remove
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
