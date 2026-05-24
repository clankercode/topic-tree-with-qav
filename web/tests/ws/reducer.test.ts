import { beforeEach, describe, expect, it } from "vitest";
import { useSessionStore } from "../../src/store";
import { applyServerMessage } from "../../src/ws/reducer";
import type { Guest, RoomSnapshot, ServerMsg } from "../../src/ws/types";

function snapshot(over: Partial<RoomSnapshot> = {}): RoomSnapshot {
  return {
    room: { id: "r1", title: "Room One", createdAt: 1000 },
    you: { clientId: "c1", role: "guest", guestId: "g1" },
    guests: [
      { guestId: "g1", displayName: "Alice", muted: false, joinedAt: 1000 },
    ],
    topics: [],
    questions: [],
    boards: [],
    hands: [],
    myVotes: [],
    focusedBoardId: null,
    activeTopicId: null,
    ...over,
  };
}

function welcome(seq = 1n, snap = snapshot()): ServerMsg {
  return {
    v: 1,
    ts: 0n,
    seq,
    type: "Welcome",
    you: snap.you,
    snapshot: snap,
  };
}

function presence(guests: Guest[], seq = 2n): ServerMsg {
  return {
    v: 1,
    ts: 0n,
    seq,
    type: "PresenceUpdate",
    guests,
  };
}

describe("ws reducer", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("Welcome populates room, me, presence, and lastSeq", () => {
    applyServerMessage(welcome(5n));
    const s = useSessionStore.getState();
    expect(s.room).toEqual({ id: "r1", title: "Room One", createdAt: 1000 });
    expect(s.me).toEqual({ clientId: "c1", role: "guest", guestId: "g1" });
    expect(s.presence).toHaveLength(1);
    expect(s.lastSeq).toBe(5n);
  });

  it("PresenceUpdate replaces guest list and advances seq", () => {
    applyServerMessage(welcome(1n));
    const newGuests: Guest[] = [
      { guestId: "g1", displayName: "Alice", muted: false, joinedAt: 1000 },
      { guestId: "g2", displayName: "Bob", muted: false, joinedAt: 2000 },
    ];
    applyServerMessage(presence(newGuests, 2n));
    const s = useSessionStore.getState();
    expect(s.presence.map((g) => g.guestId)).toEqual(["g1", "g2"]);
    expect(s.lastSeq).toBe(2n);
  });

  it("unknown server message types still advance lastSeq", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 7n,
      type: "QuestionAdded",
    } as ServerMsg);
    expect(useSessionStore.getState().lastSeq).toBe(7n);
  });
});
