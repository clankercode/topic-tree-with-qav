import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { ConnectionBanner } from "../components/ConnectionBanner";
import { PresenceIndicator } from "../components/PresenceIndicator";
import { RaiseHandButton } from "../components/RaiseHandButton";
import { RoomSessionTabs } from "../components/RoomSessionTabs";
import { SessionErrorView } from "../components/SessionErrorView";
import { ThemeToggle } from "../components/ThemeToggle";
import { getRoom } from "../lib/idb";
import { getPreviewGuest } from "../lib/previewGuest";
import { setWsClient, stopWsClient } from "../ws/manager";
import { WsClient } from "../ws/client";
import type { SortMode } from "../components/SortToggle";
import { useSessionStore } from "../store";
import { HandMetal } from "lucide-react";

interface GuestSessionView {
  title: string;
  guestId: string;
  displayName: string;
}

interface GuestSessionProps {
  preview?: boolean;
}

export function GuestSession({ preview = false }: GuestSessionProps) {
  const { roomId } = useParams();
  const [view, setView] = useState<GuestSessionView | null | undefined>(
    undefined,
  );
  const [sortMode, setSortMode] = useState<SortMode>("chronological");
  const kicked = useSessionStore((s) => s.kicked);
  const sessionError = useSessionStore((s) => s.sessionError);
  const clearSessionError = useSessionStore((s) => s.clearSessionError);
  const setConnectionStatus = useSessionStore((s) => s.setConnectionStatus);

  useEffect(() => {
    if (!roomId) {
      setView(null);
      return;
    }
    if (kicked) {
      return;
    }
    let alive = true;
    let client: WsClient | null = null;

    async function connect() {
      let session: GuestSessionView | null = null;

      if (preview) {
        const previewGuest = getPreviewGuest(roomId!);
        if (!previewGuest?.displayName) {
          if (alive) setView(null);
          return;
        }
        const room = await getRoom(roomId!);
        session = {
          title: room?.title ?? "Untitled room",
          guestId: previewGuest.guestId,
          displayName: previewGuest.displayName,
        };
      } else {
        const room = await getRoom(roomId!);
        if (!room?.guest?.displayName) {
          if (alive) setView(null);
          return;
        }
        session = {
          title: room.title,
          guestId: room.guest.guestId,
          displayName: room.guest.displayName,
        };
      }

      if (!alive || !session) return;
      setView(session);

      clearSessionError();
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//${window.location.host}/ws?room=${roomId}`;
      client = new WsClient({
        url: wsUrl,
        hello: {
          role: "guest",
          guestId: session.guestId,
          displayName: session.displayName,
        },
        onClose: () => {
          console.log("guest ws disconnected");
          setConnectionStatus("disconnected");
        },
        onError: (err) => {
          console.error("guest ws error", err);
          setConnectionStatus("disconnected");
        },
      });
      setConnectionStatus("connecting");
      client.start();
      setWsClient(client);
    }

    void connect();

    return () => {
      alive = false;
      stopWsClient();
    };
  }, [roomId, kicked, preview, setConnectionStatus, clearSessionError]);

  if (view === undefined) {
    return (
      <main className="min-h-full flex items-center justify-center p-8">
        <p className="text-[rgb(var(--muted))]">Connecting…</p>
      </main>
    );
  }

  if (!view || !roomId) {
    if (preview) {
      return <Navigate to={`/r/${roomId}/preview`} replace />;
    }
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

  if (sessionError) {
    return <SessionErrorView error={sessionError} roomId={roomId} />;
  }

  return (
    <>
      <ConnectionBanner />
      {preview ? (
        <div
          data-testid="preview-guest-banner"
          className="border-b border-[rgb(var(--border))] bg-[rgb(var(--surface))] px-4 py-2 text-center text-sm text-[rgb(var(--muted))]"
        >
          Preview mode — this tab is not saved to your rooms
        </div>
      ) : null}
      <main data-testid="guest-shell" className="min-h-full p-6">
        <div className="mx-auto space-y-4">
          <header className="mx-auto flex max-w-5xl items-center justify-between gap-4">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">
                {view.title}
              </h1>
              <p className="text-sm text-[rgb(var(--muted))]">
                Joined as {view.displayName}
              </p>
            </div>
            <div className="flex items-center gap-3">
              <ThemeToggle />
              <RaiseHandButton />
              <PresenceIndicator />
            </div>
          </header>
          <RoomSessionTabs
            sortMode={sortMode}
            onSortChange={setSortMode}
            showHandsQueue={false}
          />
        </div>
      </main>
    </>
  );
}
