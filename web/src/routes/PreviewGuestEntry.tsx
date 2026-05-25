import { FormEvent, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getRoom } from "../lib/idb";
import { createPreviewGuestId, savePreviewGuest } from "../lib/previewGuest";

export function PreviewGuestEntry() {
  const { roomId } = useParams();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [joined, setJoined] = useState(false);

  useEffect(() => {
    if (!roomId) return;
    let alive = true;
    void getRoom(roomId).then((record) => {
      if (alive && record?.title) {
        // title is shown in session; no saved preview name to restore
      }
    });
    return () => {
      alive = false;
    };
  }, [roomId]);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!roomId || !name.trim()) return;
    savePreviewGuest(roomId, {
      guestId: createPreviewGuestId(),
      displayName: name.trim(),
    });
    setJoined(true);
    navigate(`/r/${roomId}/preview/guest`, { replace: true });
  }

  return (
    <main className="min-h-full flex items-center justify-center p-8">
      <form
        onSubmit={onSubmit}
        className="w-full max-w-sm space-y-4 rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-6"
      >
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Preview as guest
          </h1>
          <p className="text-sm text-[rgb(var(--muted))]">
            Ephemeral test view — does not change your saved host access.
          </p>
        </div>
        <label className="block space-y-1">
          <span className="text-sm font-medium">Display name</span>
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
          Start preview
        </button>
        {joined ? (
          <p className="text-sm text-[rgb(var(--muted))]">Opening preview…</p>
        ) : null}
      </form>
    </main>
  );
}
