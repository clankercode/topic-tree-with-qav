import { FormEvent, useEffect, useState } from "react";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { fetchRoom } from "../lib/api";
import { getOrCreateRoomGuestId, getRoom, mergeRoomGuest } from "../lib/idb";
import { isValidRoomId } from "../lib/roomId";

type RoomCheckState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "ok"; title: string }
  | { status: "invalid" }
  | { status: "not_found" }
  | { status: "error"; message: string };

export function RoomEntry() {
  const { roomId } = useParams();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [joined, setJoined] = useState(false);
  const [roomCheck, setRoomCheck] = useState<RoomCheckState>({
    status: "idle",
  });

  useEffect(() => {
    if (!roomId) return;
    let alive = true;
    void getRoom(roomId).then((record) => {
      if (alive && record?.guest?.displayName) {
        setName(record.guest.displayName);
      }
    });
    return () => {
      alive = false;
    };
  }, [roomId]);

  useEffect(() => {
    if (!roomId) return;
    if (!isValidRoomId(roomId)) {
      setRoomCheck({ status: "invalid" });
      return;
    }
    let alive = true;
    setRoomCheck({ status: "checking" });
    void fetchRoom(roomId)
      .then((room) => {
        if (!alive) return;
        setRoomCheck({ status: "ok", title: room.title });
      })
      .catch((err: unknown) => {
        if (!alive) return;
        const message =
          err instanceof Error ? err.message : "Failed to check room";
        if (message.includes("not found")) {
          setRoomCheck({ status: "not_found" });
          return;
        }
        setRoomCheck({ status: "error", message });
      });
    return () => {
      alive = false;
    };
  }, [roomId]);

  if (!roomId?.trim()) {
    return <Navigate to="/" replace />;
  }

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!roomId || !name.trim() || roomCheck.status !== "ok") return;
    const guestId = await getOrCreateRoomGuestId(roomId);
    const now = Date.now();
    await mergeRoomGuest(roomId, {
      guestId,
      displayName: name.trim(),
      lastJoinedAt: now,
      title: roomCheck.title,
    });
    setJoined(true);
    navigate(`/r/${roomId}/guest`, { replace: true });
  }

  const joinDisabled =
    joined ||
    !name.trim() ||
    roomCheck.status === "checking" ||
    roomCheck.status !== "ok";

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
        {roomCheck.status === "invalid" ? (
          <p role="alert" className="text-sm text-red-500">
            This room link is invalid.
          </p>
        ) : null}
        {roomCheck.status === "not_found" ? (
          <p role="alert" className="text-sm text-red-500">
            Room not found. Check the link with the host.
          </p>
        ) : null}
        {roomCheck.status === "error" ? (
          <p role="alert" className="text-sm text-red-500">
            {roomCheck.message}
          </p>
        ) : null}
        {roomCheck.status === "checking" ? (
          <p className="text-sm text-[rgb(var(--muted))]">Checking room…</p>
        ) : null}
        <label className="block space-y-1">
          <span className="text-sm font-medium">Your name</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            className="w-full rounded border border-[rgb(var(--border))] bg-transparent px-3 py-2"
            autoComplete="name"
            disabled={roomCheck.status !== "ok"}
          />
        </label>
        <button
          type="submit"
          disabled={joinDisabled}
          className="w-full rounded bg-[rgb(var(--accent))] px-4 py-2 text-white disabled:opacity-60"
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
