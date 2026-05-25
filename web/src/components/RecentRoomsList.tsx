import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { listRooms, roomHasGuest, roomHasHost, type RoomRecord } from "../lib/idb";

export function RecentRoomsList({ limit = 5 }: { limit?: number }) {
  const [rooms, setRooms] = useState<RoomRecord[] | null>(null);

  useEffect(() => {
    let alive = true;
    void listRooms().then((rs) => {
      if (alive) setRooms(rs);
    });
    return () => {
      alive = false;
    };
  }, []);

  if (rooms === null) return null;
  if (rooms.length === 0) {
    return (
      <p className="text-sm text-[rgb(var(--muted))]">
        No recent rooms on this device yet.
      </p>
    );
  }
  const shown = rooms.slice(0, limit);
  return (
    <ul className="space-y-1" data-testid="recent-rooms">
      {shown.map((r) => {
        const href = roomHasHost(r)
          ? `/r/${r.roomId}/host`
          : `/r/${r.roomId}`;
        const roleLabel = roomHasHost(r)
          ? "admin"
          : roomHasGuest(r)
            ? "guest"
            : "room";
        return (
          <li
            key={r.roomId}
            className="flex items-center justify-between gap-2"
          >
            <Link to={href} className="text-[rgb(var(--accent))] underline">
              {r.title || r.roomId}
            </Link>
            <span className="text-xs uppercase tracking-wide text-[rgb(var(--muted))]">
              {roleLabel}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
