import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { BoardPanel } from "../components/BoardPanel";
import { PresenceIndicator } from "../components/PresenceIndicator";
import { QAPanel } from "../components/QAPanel";
import { RaiseHandButton } from "../components/RaiseHandButton";
import { ThemeToggle } from "../components/ThemeToggle";
import { TopicTree } from "../components/TopicTree";
import { getRoom, type RoomRecord } from "../lib/idb";
import { setWsClient } from "../ws/manager";
import { WsClient } from "../ws/client";
import type { SortMode } from "../components/SortToggle";
import { useSessionStore } from "../store";
import { HandMetal } from "lucide-react";

export function GuestSession() {
  const { roomId } = useParams();
  const [record, setRecord] = useState<RoomRecord | null | undefined>(
    undefined,
  );
  const [sortMode, setSortMode] = useState<SortMode>("chronological");
  const kicked = useSessionStore((s) => s.kicked);

  useEffect(() => {
    if (!roomId) {
      setRecord(null);
      return;
    }
    let alive = true;
    let client: WsClient | null = null;
    void getRoom(roomId).then((room) => {
      if (!alive) return;
      if (!room) {
        setRecord(null);
        return;
      }
      if (room.role !== "guest") {
        setRecord(null);
        return;
      }
      setRecord(room);

      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//${window.location.host}/ws?room=${roomId}`;
      client = new WsClient({
        url: wsUrl,
        hello: {
          role: "guest",
          guestId: room.guestId,
          displayName: room.displayName,
        },
        onOpen: () => {
          console.log("guest ws connected");
        },
        onClose: () => {
          console.log("guest ws disconnected");
          setWsClient(null);
        },
        onError: (err) => {
          console.error("guest ws error", err);
        },
      });
      client.start();
      setWsClient(client);
    });
    return () => {
      alive = false;
      setWsClient(null);
      client?.stop();
    };
  }, [roomId]);

  if (record === undefined) {
    return (
      <main className="min-h-full flex items-center justify-center p-8">
        <p className="text-[rgb(var(--muted))]">Connecting…</p>
      </main>
    );
  }

  if (!record || !roomId) {
    return <Navigate to="/" replace />;
  }

  if (kicked) {
    return (
      <main className="min-h-full flex items-center justify-center p-8">
        <div className="text-center space-y-4">
          <div className="flex justify-center">
            <HandMetal className="h-12 w-12 text-[rgb(var(--muted))]" />
          </div>
          <h1 className="text-2xl font-semibold">Removed by Host</h1>
          <p className="text-[rgb(var(--muted))]">
            You have been removed from this room.
          </p>
          <button
            onClick={() => {
              window.location.href = "/";
            }}
            className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-4 py-2 text-sm hover:bg-[rgb(var(--border))]"
          >
            Go to Home
          </button>
        </div>
      </main>
    );
  }

  return (
    <main data-testid="guest-shell" className="min-h-full p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <header className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              {record.title}
            </h1>
            <p className="text-sm text-[rgb(var(--muted))]">
              Joined as {record.displayName}
            </p>
          </div>
          <div className="flex items-center gap-3">
            <ThemeToggle />
            <RaiseHandButton />
            <PresenceIndicator />
          </div>
        </header>
        <div className="grid gap-4 lg:grid-cols-2">
          <TopicTree />
          <section className="flex max-h-[600px] min-h-[400px] flex-col rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))]">
            <QAPanel sortMode={sortMode} onSortChange={setSortMode} />
          </section>
        </div>
        <section className="flex max-h-[600px] min-h-[400px] flex-col rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] overflow-hidden">
          <BoardPanel />
        </section>
      </div>
    </main>
  );
}
