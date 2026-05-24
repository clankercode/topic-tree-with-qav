import { useEffect, useState, useRef } from "react";
import { Navigate, useParams } from "react-router-dom";
import { ActiveTopicBadge } from "../components/ActiveTopicBadge";
import { AdminBanner } from "../components/AdminBanner";
import { BoardPanel } from "../components/BoardPanel";
import { ConnectionBanner } from "../components/ConnectionBanner";
import { PresenceIndicator } from "../components/PresenceIndicator";
import { PresenceMenu } from "../components/PresenceMenu";
import { QAPanel } from "../components/QAPanel";
import { ThemeToggle } from "../components/ThemeToggle";
import { TopicTree } from "../components/TopicTree";
import { getRoom, type RoomRecord } from "../lib/idb";
import { setWsClient, sendWsMsg } from "../ws/manager";
import { useSessionStore } from "../store";
import { WsClient } from "../ws/client";
import type { SortMode } from "../components/SortToggle";
import { Hand, X } from "lucide-react";

export function HostSession() {
  const { roomId } = useParams();
  const [record, setRecord] = useState<RoomRecord | null | undefined>(
    undefined,
  );
  const [sortMode, setSortMode] = useState<SortMode>("chronological");
  const [showHandsPopup, setShowHandsPopup] = useState(false);
  const [showToast, setShowToast] = useState(false);
  const { topics, activeTopicId, hands } = useSessionStore();
  const setConnectionStatus = useSessionStore((s) => s.setConnectionStatus);
  const prevHandsCountRef = useRef(hands.length);

  useEffect(() => {
    if (hands.length > prevHandsCountRef.current) {
      setShowToast(true);
      setTimeout(() => setShowToast(false), 3000);
    }
    prevHandsCountRef.current = hands.length;
  }, [hands.length]);

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
      if (room.role !== "admin") {
        setRecord(null);
        return;
      }
      setRecord(room);

      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//${window.location.host}/ws?room=${roomId}`;
      client = new WsClient({
        url: wsUrl,
        hello: {
          role: "host",
          guestId: room.guestId,
          adminToken: room.adminToken,
        },
        onOpen: () => {
          console.log("host ws connected");
          setConnectionStatus("connected");
        },
        onClose: () => {
          console.log("host ws disconnected");
          setConnectionStatus("disconnected");
          setWsClient(null);
        },
        onError: (err) => {
          console.error("host ws error", err);
          setConnectionStatus("disconnected");
        },
      });
      client.start();
      setConnectionStatus("connecting");
      setWsClient(client);
    });
    return () => {
      alive = false;
      setWsClient(null);
      client?.stop();
    };
  }, [roomId, setConnectionStatus]);

  // Keyboard shortcuts for host: j = next pending, k = previous
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (record?.role !== "admin") return;

      const pendingTopics = topics
        .filter((t) => t.status === "pending")
        .sort((a, b) => a.ord - b.ord);

      if (pendingTopics.length === 0) return;

      if (e.key === "j") {
        e.preventDefault();
        // Find next pending topic after current active
        const currentIndex = activeTopicId
          ? pendingTopics.findIndex((t) => t.id === activeTopicId)
          : -1;
        const nextIndex = currentIndex + 1;
        if (nextIndex < pendingTopics.length) {
          // Mark current as done
          if (activeTopicId) {
            sendWsMsg({ v: 1, type: "MarkTopicDone", topicId: activeTopicId, done: true });
          }
          // Set next as active
          sendWsMsg({ v: 1, type: "SetActiveTopic", topicId: pendingTopics[nextIndex].id });
        }
      }

      if (e.key === "k") {
        e.preventDefault();
        // Find previous pending topic before current active
        const currentIndex = activeTopicId
          ? pendingTopics.findIndex((t) => t.id === activeTopicId)
          : 0;
        const prevIndex = currentIndex - 1;
        if (prevIndex >= 0) {
          // Unmark current as done
          if (activeTopicId) {
            sendWsMsg({ v: 1, type: "MarkTopicDone", topicId: activeTopicId, done: false });
          }
          // Set previous as active
          sendWsMsg({ v: 1, type: "SetActiveTopic", topicId: pendingTopics[prevIndex].id });
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [topics, activeTopicId, record?.role]);

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
    <>
      <ConnectionBanner />
    <main data-testid="host-shell" className="min-h-full p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <header className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-4">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">
                {record.title}
              </h1>
              <p className="text-sm text-[rgb(var(--muted))]">Host view</p>
            </div>
            <ActiveTopicBadge />
          </div>
          <div className="flex items-center gap-4">
            <div className="relative">
              <button
                onClick={() => setShowHandsPopup(!showHandsPopup)}
                className={`flex items-center gap-2 rounded px-3 py-2 transition-all ${
                  hands.length > 0
                    ? "bg-[rgb(var(--accent))] text-white animate-pulse"
                    : "border border-[rgb(var(--border))] text-[rgb(var(--muted))] hover:border-[rgb(var(--accent))]"
                }`}
                aria-label="Raised hands"
              >
                <Hand size={18} />
                {hands.length > 0 && (
                  <span className="text-lg font-bold">{hands.length}</span>
                )}
              </button>
              {showToast && (
                <div className="absolute top-full right-0 mt-2 px-4 py-2 bg-[rgb(var(--accent))] text-white rounded-lg shadow-lg animate-bounce z-50 whitespace-nowrap">
                  New hand raised!
                </div>
              )}
              {showHandsPopup && (
                <div className="absolute top-full right-0 mt-2 w-80 max-h-96 overflow-y-auto bg-[rgb(var(--surface))] border border-[rgb(var(--border))] rounded-lg shadow-xl z-50">
                  <div className="sticky top-0 flex items-center justify-between p-3 border-b border-[rgb(var(--border))] bg-[rgb(var(--surface))]">
                    <span className="font-medium text-sm">Raised Hands ({hands.length})</span>
                    <button
                      onClick={() => setShowHandsPopup(false)}
                      className="text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
                    >
                      <X size={16} />
                    </button>
                  </div>
                  <div className="p-2">
                    {hands.length === 0 ? (
                      <p className="py-4 text-center text-xs text-[rgb(var(--muted))]">No raised hands.</p>
                    ) : (
                      <div className="flex flex-col gap-2">
                        {[...hands].sort((a, b) => a.raisedAt - b.raisedAt).map((hand) => (
                          <div
                            key={hand.guestId}
                            className="flex items-start gap-2 rounded border border-[rgb(var(--border))] bg-[rgb(var(--bg))] p-2"
                          >
                            <div className="flex flex-1 flex-col gap-0.5">
                              <span className="text-xs text-[rgb(var(--muted))]">{hand.displayName}</span>
                              <p className="text-sm">{hand.topic}</p>
                            </div>
                            <div className="flex gap-1">
                              <button
                                onClick={() => {
                                  sendWsMsg({ v: 1, type: "CallOnHand", guestId: hand.guestId });
                                }}
                                className="rounded p-1 text-[rgb(var(--success))] hover:bg-[rgb(var(--success))]/10"
                                title="Call on"
                              >
                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="20 6 9 17 4 12" /></svg>
                              </button>
                              <button
                                onClick={() => {
                                  sendWsMsg({ v: 1, type: "DismissHand", guestId: hand.guestId });
                                }}
                                className="rounded p-1 text-[rgb(var(--muted))] hover:bg-red-500/10 hover:text-red-500"
                                title="Dismiss"
                              >
                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M18 6L6 18M6 6l12 12" /></svg>
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
            <ThemeToggle />
            <PresenceMenu />
            <PresenceIndicator />
          </div>
        </header>
        <AdminBanner joinUrl={joinUrl} adminUrl={adminUrl} />
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
    </>
  );
}
