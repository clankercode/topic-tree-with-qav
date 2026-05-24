import { useEffect, useState } from "react";
import { Navigate, useParams } from "react-router-dom";
import { ActiveTopicBadge } from "../components/ActiveTopicBadge";
import { AdminBanner } from "../components/AdminBanner";
import { BoardPanel } from "../components/BoardPanel";
import { PresenceIndicator } from "../components/PresenceIndicator";
import { PresenceMenu } from "../components/PresenceMenu";
import { QAPanel } from "../components/QAPanel";
import { TopicTree } from "../components/TopicTree";
import { getRoom, type RoomRecord } from "../lib/idb";
import { setWsClient, sendWsMsg } from "../ws/manager";
import { useSessionStore } from "../store";
import { WsClient } from "../ws/client";
import type { SortMode } from "../components/SortToggle";

export function HostSession() {
  const { roomId } = useParams();
  const [record, setRecord] = useState<RoomRecord | null | undefined>(
    undefined,
  );
  const [sortMode, setSortMode] = useState<SortMode>("chronological");
  const { topics, activeTopicId } = useSessionStore();

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
        },
        onClose: () => {
          console.log("host ws disconnected");
          setWsClient(null);
        },
        onError: (err) => {
          console.error("host ws error", err);
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
  );
}
