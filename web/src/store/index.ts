import { create } from "zustand";
import type { Guest, RoomSnapshot, RoomSummary } from "../ws/types";
import type { Role } from "../proto/generated";

export interface Me {
  clientId: string;
  role: Role;
  guestId: string;
}

export interface SessionState {
  room: RoomSummary | null;
  me: Me | null;
  presence: Guest[];
  lastSeq: bigint | null;
  applyWelcome(snapshot: RoomSnapshot, seq: bigint): void;
  applyPresence(guests: Guest[], seq: bigint): void;
  setLastSeq(seq: bigint): void;
  reset(): void;
}

export const useSessionStore = create<SessionState>((set) => ({
  room: null,
  me: null,
  presence: [],
  lastSeq: null,
  applyWelcome(snapshot, seq) {
    set({
      room: snapshot.room,
      me: {
        clientId: snapshot.you.clientId,
        role: snapshot.you.role,
        guestId: snapshot.you.guestId,
      },
      presence: snapshot.guests,
      lastSeq: seq,
    });
  },
  applyPresence(guests, seq) {
    set({ presence: guests, lastSeq: seq });
  },
  setLastSeq(seq) {
    set({ lastSeq: seq });
  },
  reset() {
    set({ room: null, me: null, presence: [], lastSeq: null });
  },
}));
