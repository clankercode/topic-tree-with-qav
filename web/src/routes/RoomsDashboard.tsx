import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { listRooms, roomHasGuest, roomHasHost, type RoomRecord } from "../lib/idb";

export function RoomsDashboard() {
  const [rooms, setRooms] = useState<RoomRecord[] | null>(null);

  useEffect(() => {
    let alive = true;
    void listRooms().then((records) => {
      if (alive) setRooms(records);
    });
    return () => {
      alive = false;
    };
  }, []);

  return (
    <main className="min-h-full p-8">
      <div className="mx-auto max-w-3xl space-y-4">
        <header className="flex items-center justify-between">
          <h1 className="text-3xl font-semibold tracking-tight">Your rooms</h1>
          <Link to="/" className="text-sm text-[rgb(var(--accent))] underline">
            Create room
          </Link>
        </header>
        {rooms === null ? (
          <p className="text-sm text-[rgb(var(--muted))]">Loading rooms…</p>
        ) : rooms.length === 0 ? (
          <p className="text-sm text-[rgb(var(--muted))]">
            No rooms saved on this device yet.
          </p>
        ) : (
          <ul className="divide-y divide-[rgb(var(--border))] rounded border border-[rgb(var(--border))]">
            {rooms.map((room) => {
              const href = roomHasHost(room)
                ? `/r/${room.roomId}/host`
                : `/r/${room.roomId}`;
              const label = roomHasHost(room)
                ? "Host"
                : roomHasGuest(room)
                  ? "Guest"
                  : "Room";
              return (
                <li key={room.roomId} className="p-4">
                  <Link
                    to={href}
                    className="font-medium text-[rgb(var(--accent))]"
                  >
                    {room.title || room.roomId}
                  </Link>
                  <p className="text-sm text-[rgb(var(--muted))]">{label}</p>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </main>
  );
}
