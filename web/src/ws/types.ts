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

export interface RoomSummary {
  id: string;
  title: string;
  createdAt: number;
}

export interface RoomSnapshot {
  room: RoomSummary;
  you: You & { guestId: string };
  guests: Guest[];
  topics: Topic[];
  questions: unknown[];
  boards: unknown[];
  hands: unknown[];
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
  | { v: 1; id?: string; type: string; [k: string]: unknown };
