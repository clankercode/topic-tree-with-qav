import { FormEvent, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { getOrCreateGuestId, getRoom, upsertRoom } from "../lib/idb";

export function RoomEntry() {
  const { roomId } = useParams();
  const [name, setName] = useState("");
  const [joined, setJoined] = useState(false);

  useEffect(() => {
    if (!roomId) return;
    let alive = true;
    void getRoom(roomId).then((record) => {
      if (alive && record?.displayName) setName(record.displayName);
    });
    return () => {
      alive = false;
    };
  }, [roomId]);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!roomId || !name.trim()) return;
    const guestId = await getOrCreateGuestId();
    const now = Date.now();
    await upsertRoom({
      roomId,
      title: "Untitled room",
      role: "guest",
      guestId,
      displayName: name.trim(),
      createdAt: now,
      lastJoinedAt: now,
    });
    setJoined(true);
  }

  return (
    <main className="min-h-full flex items-center justify-center p-8">
      <form
        onSubmit={onSubmit}
        className="w-full max-w-sm space-y-4 rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-6"
      >
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">Join room</h1>
          <p className="text-sm text-[rgb(var(--muted))]">
            Enter the display name other attendees will see.
          </p>
        </div>
        <label className="block space-y-1">
          <span className="text-sm font-medium">Your name</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            className="w-full rounded border border-[rgb(var(--border))] bg-transparent px-3 py-2"
            autoComplete="name"
          />
        </label>
        <button
          type="submit"
          className="w-full rounded bg-[rgb(var(--accent))] px-4 py-2 text-white"
        >
          Join
        </button>
        {joined ? (
          <p className="text-sm text-[rgb(var(--muted))]">Joining session…</p>
        ) : null}
      </form>
    </main>
  );
}
