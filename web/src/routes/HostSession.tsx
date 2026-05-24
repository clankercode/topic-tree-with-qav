import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { AdminBanner } from "../components/AdminBanner";
import { PresenceIndicator } from "../components/PresenceIndicator";
import { getRoom, type RoomRecord } from "../lib/idb";

export function HostSession() {
  const { roomId } = useParams();
  const [record, setRecord] = useState<RoomRecord | null | undefined>(
    undefined,
  );

  useEffect(() => {
    if (!roomId) {
      setRecord(null);
      return;
    }
    let alive = true;
    void getRoom(roomId).then((room) => {
      if (alive) setRecord(room?.role === "admin" ? room : null);
    });
    return () => {
      alive = false;
    };
  }, [roomId]);

  if (record === undefined) {
    return (
      <main className="min-h-full flex items-center justify-center p-8">
        <p className="text-[rgb(var(--muted))]">Loading room…</p>
      </main>
    );
  }

  if (!record || !roomId) return <Navigate to="/" replace />;

  const joinUrl = `${window.location.origin}/r/${roomId}`;
  const adminUrl = `${joinUrl}?admin=${encodeURIComponent(record.adminToken ?? "")}`;

  return (
    <main data-testid="host-shell" className="min-h-full p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <header className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              {record.title}
            </h1>
            <p className="text-sm text-[rgb(var(--muted))]">Host view</p>
          </div>
          <PresenceIndicator />
        </header>
        <AdminBanner joinUrl={joinUrl} adminUrl={adminUrl} />
        <section className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-6">
          <h2 className="text-lg font-medium">Session ready</h2>
          <p className="text-sm text-[rgb(var(--muted))]">
            Room connection and live presence will appear here.
          </p>
        </section>
      </div>
    </main>
  );
}
