import { beforeEach, describe, expect, it } from "vitest";
import { useSessionStore } from "../../src/store";
import { applyServerMessage } from "../../src/ws/reducer";
import type {
  Guest,
  Question,
  RaisedHand,
  RoomSnapshot,
  ServerMsg,
} from "../../src/ws/types";

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

function makeTopic(
  id: string,
  title: string,
  parentId: string | null = null,
): import("../../src/ws/types").Topic {
  return {
    id,
    parentId,
    title,
    ord: 1.0,
    status: "pending" as const,
    createdAt: 1000,
  };
}

function makeQuestion(id: string, text: string): Question {
  return {
    id,
    roomId: "r1",
    authorGuestId: "g1",
    authorName: "Alice",
    anonymous: false,
    text,
    answered: false,
    createdAt: 1000,
    voteCount: 0,
  };
}

function makeHand(guestId: string, topic: string): RaisedHand {
  return { guestId, displayName: "Alice", topic, raisedAt: 1000 };
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

  it("TopicTreeUpdated replaces topics and activeTopicId", () => {
    applyServerMessage(welcome(1n));
    const topics = [
      makeTopic("t1", "Topic 1"),
      makeTopic("t2", "Topic 2", "t1"),
    ];
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "TopicTreeUpdated",
      topics,
      activeTopicId: "t1",
    });
    const s = useSessionStore.getState();
    expect(s.topics).toHaveLength(2);
    expect(s.activeTopicId).toBe("t1");
    expect(s.lastSeq).toBe(3n);
  });

  it("QuestionAdded appends to questions list", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "QuestionAdded",
      question: makeQuestion("q1", "What is Rust?"),
    });
    const s = useSessionStore.getState();
    expect(s.questions).toHaveLength(1);
    expect(s.questions[0].text).toBe("What is Rust?");
  });

  it("QuestionUpdated replaces existing question", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "QuestionAdded",
      question: makeQuestion("q1", "What is Rust?"),
    });
    const updated = { ...makeQuestion("q1", "What is Rust?"), answered: true };
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "QuestionUpdated",
      question: updated,
    });
    const s = useSessionStore.getState();
    expect(s.questions[0].answered).toBe(true);
  });

  it("QuestionDeleted removes question from list", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "QuestionAdded",
      question: makeQuestion("q1", "What is Rust?"),
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "QuestionDeleted",
      questionId: "q1",
    });
    const s = useSessionStore.getState();
    expect(s.questions).toHaveLength(0);
  });

  it("VoteUpdated changes vote count and myVotes", () => {
    applyServerMessage(
      welcome(
        1n,
        snapshot({
          myVotes: [],
        }),
      ),
    );
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "QuestionAdded",
      question: makeQuestion("q1", "What is Rust?"),
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "VoteUpdated",
      questionId: "q1",
      voteCount: 1,
      voterGuestId: "g1",
    });
    const s = useSessionStore.getState();
    expect(s.questions[0].voteCount).toBe(1);
    expect(s.myVotes.has("q1")).toBe(true);
  });

  it("BoardCreated adds board to list", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Board 1",
        createdAt: 1000,
      },
    });
    const s = useSessionStore.getState();
    expect(s.boards).toHaveLength(1);
    expect(s.boards[0].id).toBe("b1");
  });

  it("BoardUpdated updates board properties", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Board 1",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "BoardUpdated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Renamed Board",
        createdAt: 1000,
      },
    });
    const s = useSessionStore.getState();
    expect(s.boards[0].title).toBe("Renamed Board");
  });

  it("BoardDeleted removes board from list", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Board 1",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "BoardDeleted",
      boardId: "b1",
    });
    const s = useSessionStore.getState();
    expect(s.boards).toHaveLength(0);
  });

  it("FocusedBoardChanged updates focusedBoardId for host", () => {
    applyServerMessage(
      welcome(
        1n,
        snapshot({
          you: { clientId: "c1", role: "host", guestId: "h1" },
        }),
      ),
    );
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "FocusedBoardChanged",
      boardId: "b1",
    });
    const s = useSessionStore.getState();
    expect(s.focusedBoardId).toBe("b1");
  });

  it("FocusedBoardChanged does not update for guest not following host", async () => {
    const { useFollowHostStore } = await import("../../src/store/followHost");
    useFollowHostStore.setState({ followingHost: false });
    applyServerMessage(welcome(1n));
    const initial = useSessionStore.getState().focusedBoardId;
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "FocusedBoardChanged",
      boardId: "b1",
    });
    const s = useSessionStore.getState();
    expect(s.focusedBoardId).toBe(initial);
    useFollowHostStore.setState({ followingHost: true });
  });

  it("ExcalidrawDelta updates board scene version and elements", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "excalidraw",
        title: "Excal",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "ExcalidrawDelta",
      boardId: "b1",
      sceneVersion: 2,
      elements: [{ id: "el1" }],
      appState: { viewModeEnabled: false },
    });
    const s = useSessionStore.getState();
    const board = s.boards.find(
      (b) => b.id === "b1",
    ) as import("../../src/ws/types").ExcalidrawBoard;
    expect(board.sceneVersion).toBe(2);
    expect(board.elements).toEqual([{ id: "el1" }]);
  });

  it("ExcalidrawSceneReset replaces scene entirely", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "excalidraw",
        title: "Excal",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "ExcalidrawDelta",
      boardId: "b1",
      sceneVersion: 5,
      elements: [{ id: "old" }],
      appState: {},
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 4n,
      type: "ExcalidrawSceneReset",
      boardId: "b1",
      sceneVersion: 1,
      elements: [{ id: "new" }],
      appState: { viewModeEnabled: true },
    });
    const s = useSessionStore.getState();
    const board = s.boards.find(
      (b) => b.id === "b1",
    ) as import("../../src/ws/types").ExcalidrawBoard;
    expect(board.sceneVersion).toBe(1);
  });

  it("PenStrokeBegun adds in-progress stroke", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenStrokeBegun",
      boardId: "b1",
      strokeId: "s1",
      color: "#000",
      size: 4,
    });
    const s = useSessionStore.getState();
    const key = `${"b1"}:${"s1"}`;
    expect(s.penInProgressStrokes.has(key)).toBe(true);
  });

  it("PenStrokeAppended extends stroke points", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenStrokeBegun",
      boardId: "b1",
      strokeId: "s1",
      color: "#000",
      size: 4,
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 4n,
      type: "PenStrokeAppended",
      boardId: "b1",
      strokeId: "s1",
      points: [
        [0, 1, 2],
        [3, 4, 5],
      ],
    });
    const s = useSessionStore.getState();
    const key = `${"b1"}:${"s1"}`;
    expect(s.penInProgressStrokes.get(key)?.points).toHaveLength(2);
  });

  it("PenStrokeEnded moves stroke to penBoards", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenStrokeBegun",
      boardId: "b1",
      strokeId: "s1",
      color: "#000",
      size: 4,
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 4n,
      type: "PenStrokeEnded",
      boardId: "b1",
      strokeId: "s1",
    });
    const s = useSessionStore.getState();
    expect(s.penInProgressStrokes.has(`${"b1"}:${"s1"}`)).toBe(false);
    const board = s.penBoards.get("b1");
    expect(board?.strokes).toHaveLength(1);
  });

  it("host ignores PenStrokeBegun echo so local points are not wiped", () => {
    applyServerMessage(
      welcome(1n, snapshot({ you: { clientId: "c1", role: "host", guestId: "h1" } })),
    );
    const key = "b1:s1";
    useSessionStore.setState({
      penInProgressStrokes: new Map([
        [
          key,
          {
            color: "#000",
            size: 4,
            points: [[10, 20, 0.5] as [number, number, number]],
          },
        ],
      ]),
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "PenStrokeBegun",
      boardId: "b1",
      strokeId: "s1",
      color: "#000",
      size: 4,
      authorClientId: "c1",
    });
    const s = useSessionStore.getState();
    expect(s.penInProgressStrokes.get(key)?.points).toEqual([[10, 20, 0.5]]);
  });

  it("host ignores PenStrokeAppended echo so points are not duplicated", () => {
    applyServerMessage(
      welcome(1n, snapshot({ you: { clientId: "c1", role: "host", guestId: "h1" } })),
    );
    const key = "b1:s1";
    useSessionStore.setState({
      penInProgressStrokes: new Map([
        [
          key,
          {
            color: "#000",
            size: 4,
            points: [[10, 20, 0.5] as [number, number, number]],
          },
        ],
      ]),
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "PenStrokeAppended",
      boardId: "b1",
      strokeId: "s1",
      points: [[30, 40, 0.6] as [number, number, number]],
    });
    const s = useSessionStore.getState();
    expect(s.penInProgressStrokes.get(key)?.points).toHaveLength(1);
  });

  it("host still applies PenStrokeEnded to finalize stroke", () => {
    applyServerMessage(
      welcome(1n, snapshot({ you: { clientId: "c1", role: "host", guestId: "h1" } })),
    );
    const key = "b1:s1";
    useSessionStore.setState({
      penInProgressStrokes: new Map([
        [
          key,
          {
            color: "#000",
            size: 4,
            points: [[10, 20, 0.5] as [number, number, number]],
          },
        ],
      ]),
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "PenStrokeEnded",
      boardId: "b1",
      strokeId: "s1",
    });
    const s = useSessionStore.getState();
    expect(s.penInProgressStrokes.has(key)).toBe(false);
    expect(s.penBoards.get("b1")?.strokes).toHaveLength(1);
  });

  it("PenTextUpserted adds or updates text", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenTextUpserted",
      boardId: "b1",
      text: {
        id: "t1",
        text: "Hello",
        x: 10,
        y: 20,
        fontSize: 16,
        color: "#000",
      },
    });
    const s = useSessionStore.getState();
    const board = s.penBoards.get("b1");
    expect(board?.texts).toHaveLength(1);
    expect(board?.texts[0].text).toBe("Hello");
  });

  it("PenTextDeleted removes text", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenTextUpserted",
      boardId: "b1",
      text: {
        id: "t1",
        text: "Hello",
        x: 10,
        y: 20,
        fontSize: 16,
        color: "#000",
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 4n,
      type: "PenTextDeleted",
      boardId: "b1",
      textId: "t1",
    });
    const s = useSessionStore.getState();
    const board = s.penBoards.get("b1");
    expect(board?.texts).toHaveLength(0);
  });

  it("PenCleared removes all strokes and texts", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "BoardCreated",
      board: {
        id: "b1",
        roomId: "r1",
        kind: "pen",
        title: "Pen",
        createdAt: 1000,
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 3n,
      type: "PenTextUpserted",
      boardId: "b1",
      text: {
        id: "t1",
        text: "Hello",
        x: 10,
        y: 20,
        fontSize: 16,
        color: "#000",
      },
    });
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 4n,
      type: "PenCleared",
      boardId: "b1",
    });
    const s = useSessionStore.getState();
    const board = s.penBoards.get("b1");
    expect(board?.strokes).toHaveLength(0);
    expect(board?.texts).toHaveLength(0);
  });

  it("HandsUpdated replaces hands list", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "HandsUpdated",
      hands: [makeHand("g1", "Question 1"), makeHand("g2", "Question 2")],
    });
    const s = useSessionStore.getState();
    expect(s.hands).toHaveLength(2);
  });

  it("KickNotice sets kicked flag when guestId matches the local guest", () => {
    applyServerMessage(welcome(1n));
    expect(useSessionStore.getState().kicked).toBe(false);
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "KickNotice",
      guestId: "g1",
    });
    expect(useSessionStore.getState().kicked).toBe(true);
  });

  it("KickNotice for a different guest is ignored", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 2n,
      type: "KickNotice",
      guestId: "someone-else",
    });
    expect(useSessionStore.getState().kicked).toBe(false);
  });

  it("unknown server message types still advance lastSeq", () => {
    applyServerMessage(welcome(1n));
    applyServerMessage({
      v: 1,
      ts: 0n,
      seq: 7n,
      type: "UnknownMessageType",
    } as ServerMsg);
    expect(useSessionStore.getState().lastSeq).toBe(7n);
  });
});
