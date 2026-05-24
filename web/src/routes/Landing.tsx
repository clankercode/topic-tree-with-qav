import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { createRoom, persistCreatedRoomAsAdmin } from "../lib/api";
import { RecentRoomsList } from "../components/RecentRoomsList";

export function Landing() {
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onCreate() {
    setBusy(true);
    setError(null);
    try {
      const room = await createRoom({});
      await persistCreatedRoomAsAdmin(room);
      navigate(`/r/${room.roomId}/host`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create room");
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <header className="flex justify-end px-6 py-3 border-b border-[rgb(var(--border))]">
        <a
          href="https://clankercode.github.io/topic-tree-with-qav/"
          target="_blank"
          rel="noopener noreferrer"
          className="text-sm text-[rgb(var(--accent))] hover:underline"
        >
          Docs
        </a>
      </header>
    <main className="min-h-full flex items-center justify-center p-8">
      <div className="max-w-xl w-full space-y-6 text-center">
        <h1 className="text-4xl font-semibold tracking-tight">
          topic-tree-with-qav
        </h1>
        <p className="text-[rgb(var(--muted))]">
          Host-led, audience-interactive sessions.
        </p>
        <div>
          <button
            type="button"
            onClick={onCreate}
            disabled={busy}
            className="px-4 py-2 rounded bg-[rgb(var(--accent))] text-white disabled:opacity-60"
          >
            {busy ? "Creating…" : "Create room"}
          </button>
        </div>
        {error ? (
          <p role="alert" className="text-sm text-red-500">
            {error}
          </p>
        ) : null}
        <section className="text-left space-y-2">
          <h2 className="text-sm font-medium uppercase tracking-wide text-[rgb(var(--muted))]">
            Recent rooms
          </h2>
          <RecentRoomsList />
        </section>
      </div>
    </main>
    </>
  );
}
