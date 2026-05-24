import type { Role, You } from "../proto/generated";

export interface Guest {
  guestId: string;
  displayName: string;
  muted: boolean;
  joinedAt: number;
}

export type TopicStatus = "pending" | "done";

export interface Topic {
  id: string;
  parentId: string | null;
  title: string;
  ord: number;
  status: TopicStatus;
  createdAt: number;
}

export interface Question {
  id: string;
  roomId: string;
  authorGuestId: string;
  authorName: string;
  anonymous: boolean;
  text: string;
  answered: boolean;
  createdAt: number;
  voteCount: number;
}

export type BoardKind = "pen" | "excalidraw";

export interface Board {
  id: string;
  kind: BoardKind;
  title: string;
  ord: number;
  createdAt: number;
}

export interface PenBoard extends Board {
  kind: "pen";
  content: PenBoardContent;
}

export interface PenText {
  id: string;
  x: number;
  y: number;
  text: string;
  fontSize: number;
  color: string;
  updatedAt: number;
}

export interface PenStrokeSummary {
  id: string;
  color: string;
  size: number;
  points: [number, number, number][];
  createdAt: number;
  ord: number;
}

export interface PenBoardContent {
  strokes: PenStrokeSummary[];
  texts: PenText[];
}

export interface ExcalidrawBoard extends Board {
  kind: "excalidraw";
  sceneVersion: number;
  elements: unknown[];
  appState: unknown;
}

export type FatBoard = Board | ExcalidrawBoard | PenBoard;

export interface RoomSummary {
  id: string;
  title: string;
  createdAt: number;
}

export interface RaisedHand {
  guestId: string;
  displayName: string;
  topic: string;
  raisedAt: number;
}

export interface RoomSnapshot {
  room: RoomSummary;
  you: You & { guestId: string };
  guests: Guest[];
  topics: Topic[];
  questions: Question[];
  boards: FatBoard[];
  hands: RaisedHand[];
  myVotes: string[];
  focusedBoardId: string | null;
  activeTopicId: string | null;
}

interface Envelope {
  v: number;
  ts: bigint;
  seq: bigint;
}

export type ServerMsg =
  | (Envelope & {
      type: "Welcome";
      you: You & { guestId: string };
      snapshot: RoomSnapshot;
    })
  | (Envelope & { type: "PresenceUpdate"; guests: Guest[] })
  | (Envelope & { type: "Ping" })
  | (Envelope & { type: "Ack"; refId: string })
  | (Envelope & {
      type: "Error";
      code: string;
      message: string;
      refId?: string;
    })
  | (Envelope & { type: "TopicTreeUpdated"; topics: Topic[]; activeTopicId: string | null })
  | (Envelope & { type: "QuestionAdded"; question: Question })
  | (Envelope & { type: "QuestionUpdated"; question: Question })
  | (Envelope & { type: "QuestionDeleted"; questionId: string })
  | (Envelope & { type: "VoteUpdated"; questionId: string; voteCount: number; voterGuestId: string })
  | (Envelope & { type: "BoardCreated"; board: Board })
  | (Envelope & { type: "BoardUpdated"; board: Board })
  | (Envelope & { type: "BoardDeleted"; boardId: string })
  | (Envelope & { type: "FocusedBoardChanged"; boardId: string })
  | (Envelope & { type: "ExcalidrawDelta"; boardId: string; sceneVersion: number; elements: unknown[]; appState: unknown })
  | (Envelope & { type: "ExcalidrawSceneReset"; boardId: string; sceneVersion: number; elements: unknown[]; appState: unknown })
  | (Envelope & { type: "PenStrokeBegun"; boardId: string; strokeId: string; color: string; size: number })
  | (Envelope & { type: "PenStrokeAppended"; boardId: string; strokeId: string; points: [number, number, number][] })
  | (Envelope & { type: "PenStrokeEnded"; boardId: string; strokeId: string })
  | (Envelope & { type: "PenTextUpserted"; boardId: string; text: PenText })
  | (Envelope & { type: "PenTextDeleted"; boardId: string; textId: string })
  | (Envelope & { type: "PenCleared"; boardId: string })
  | (Envelope & { type: "PenUndone"; boardId: string; removedStrokeId: string | null; removedTextId: string | null })
  | (Envelope & { type: "CursorMoved"; boardId: string; clientId: string; guestId: string; displayName: string; x: number; y: number })
  | (Envelope & { type: "Clicked"; boardId: string; clientId: string; guestId: string; displayName: string; x: number; y: number })
  | (Envelope & { type: "HandsUpdated"; hands: RaisedHand[] })
  | (Envelope & { type: "QuestionPromotedToTopic"; questionId: string; topic: Topic })
  | (Envelope & { type: "RoomSnapshot"; snapshot: RoomSnapshot })
  | (Envelope & { type: "KickNotice" })
  | (Envelope & { type: string; [k: string]: unknown });

export interface ClientHello {
  v: 1;
  type: "Hello";
  role: Role;
  guestId: string;
  displayName?: string;
  adminToken?: string;
}

export type ClientMsg =
  | ClientHello
  | { v: 1; type: "Pong" }
  | { v: 1; type: "GetSnapshot"; since?: number }
  | { v: 1; id?: string; type: "KickGuest"; guestId: string }
  | { v: 1; id?: string; type: "MuteGuest"; guestId: string; muted: boolean }
  | { v: 1; id?: string; type: "RaiseHand"; topic: string }
  | { v: 1; id?: string; type: "LowerHand" }
  | { v: 1; id?: string; type: "CallOnHand"; guestId: string }
  | { v: 1; id?: string; type: "DismissHand"; guestId: string }
  | { v: 1; id?: string; type: "PromoteQuestionToTopic"; questionId: string; parentTopicId?: string; afterTopicId?: string }
  | { v: 1; id?: string; type: "Cursor"; boardId: string; x: number; y: number }
  | { v: 1; id?: string; type: "Click"; boardId: string; x: number; y: number }
  | { v: 1; id?: string; type: string; [k: string]: unknown };
