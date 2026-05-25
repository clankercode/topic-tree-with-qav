import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSessionStore } from "../../src/store";
import { WsClient } from "../../src/ws/client";
import type { WebSocketLike } from "../../src/ws/client";

interface FakeSocket extends WebSocketLike {
  readonly url: string;
  readonly sent: string[];
  closed: boolean;
  open(): void;
  receive(text: string): void;
  remoteClose(code?: number, reason?: string): void;
}

function makeFakeSocket(url: string): FakeSocket {
  const fake: FakeSocket = {
    url,
    sent: [],
    closed: false,
    readyState: 0,
    OPEN: 1,
    onopen: null,
    onmessage: null,
    onclose: null,
    onerror: null,
    send(data: string) {
      fake.sent.push(data);
    },
    close() {
      fake.closed = true;
      (fake as { readyState: number }).readyState = 3;
    },
    open() {
      (fake as { readyState: number }).readyState = 1;
      fake.onopen?.(new Event("open"));
    },
    receive(text: string) {
      fake.onmessage?.(new MessageEvent("message", { data: text }));
    },
    remoteClose(code = 1006, reason = "") {
      fake.closed = true;
      (fake as { readyState: number }).readyState = 3;
      fake.onclose?.(
        new CloseEvent("close", { code, reason, wasClean: false }),
      );
    },
  };
  return fake;
}

function sentTypes(socket: FakeSocket): string[] {
  return socket.sent.map((s) => (JSON.parse(s) as { type: string }).type);
}

function serverEnvelope(extra: Record<string, unknown>): string {
  return JSON.stringify({ v: 1, ts: 0, ...extra });
}

function welcomeJson(seq: number) {
  return serverEnvelope({
    type: "Welcome",
    seq,
    you: { clientId: "c1", role: "guest", guestId: "g1" },
    snapshot: {
      room: { id: "r1", title: "Room One", createdAt: 1000 },
      you: { clientId: "c1", role: "guest", guestId: "g1" },
      guests: [],
      topics: [],
      questions: [],
      boards: [],
      hands: [],
      myVotes: [],
      focusedBoardId: null,
      activeTopicId: null,
    },
  });
}

describe("WsClient", () => {
  const sockets: FakeSocket[] = [];

  beforeEach(() => {
    vi.useFakeTimers();
    sockets.length = 0;
    useSessionStore.getState().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeClient() {
    return new WsClient({
      url: "ws://test/ws?room=r1",
      hello: { role: "guest", guestId: "g1", displayName: "Alice" },
      socketFactory: (url) => {
        const s = makeFakeSocket(url);
        sockets.push(s);
        return s;
      },
    });
  }

  it("sends Hello on open", () => {
    const client = makeClient();
    client.start();
    sockets[0].open();
    expect(sentTypes(sockets[0])).toEqual(["Hello"]);
    const hello = JSON.parse(sockets[0].sent[0]);
    expect(hello).toMatchObject({
      v: 1,
      type: "Hello",
      role: "guest",
      guestId: "g1",
      displayName: "Alice",
    });
    client.stop();
  });

  it("replies Pong to Ping", () => {
    const client = makeClient();
    client.start();
    sockets[0].open();
    sockets[0].receive(serverEnvelope({ type: "Ping", seq: 1 }));
    expect(sentTypes(sockets[0])).toEqual(["Hello", "Pong"]);
    client.stop();
  });

  it("sends GetSnapshot when seq has a gap", () => {
    const client = makeClient();
    client.start();
    sockets[0].open();
    sockets[0].receive(welcomeJson(5));
    sockets[0].receive(
      serverEnvelope({ type: "PresenceUpdate", seq: 7, guests: [] }),
    );
    expect(sentTypes(sockets[0])).toEqual(["Hello", "GetSnapshot"]);
    client.stop();
  });

  it("does not request snapshot when seq is exactly previous + 1", () => {
    const client = makeClient();
    client.start();
    sockets[0].open();
    sockets[0].receive(welcomeJson(5));
    sockets[0].receive(
      serverEnvelope({ type: "PresenceUpdate", seq: 6, guests: [] }),
    );
    expect(sentTypes(sockets[0])).toEqual(["Hello"]);
    client.stop();
  });

  it("reconnects with 1s/2s/4s exponential backoff capped at 30s", () => {
    const client = makeClient();
    client.start();
    expect(sockets).toHaveLength(1);

    sockets[0].open();
    sockets[0].remoteClose();
    vi.advanceTimersByTime(999);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);

    sockets[1].open();
    sockets[1].remoteClose();
    vi.advanceTimersByTime(2000);
    expect(sockets).toHaveLength(3);

    sockets[2].open();
    sockets[2].remoteClose();
    vi.advanceTimersByTime(4000);
    expect(sockets).toHaveLength(4);

    for (let i = 3; i < 8; i++) {
      sockets[i].open();
      sockets[i].remoteClose();
      vi.advanceTimersByTime(60_000);
    }
    expect(sockets.length).toBeGreaterThanOrEqual(8);

    sockets[sockets.length - 1].remoteClose();
    const before = sockets.length;
    vi.advanceTimersByTime(29_999);
    expect(sockets).toHaveLength(before);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(before + 1);

    client.stop();
  });

  it("stop() prevents further reconnects", () => {
    const client = makeClient();
    client.start();
    sockets[0].open();
    client.stop();
    sockets[0].remoteClose();
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
  });
});
