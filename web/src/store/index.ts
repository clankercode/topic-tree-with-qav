import { create } from "zustand";
import type { Guest, Question, RoomSnapshot, RoomSummary, Topic } from "../ws/types";
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
  topics: Topic[];
  activeTopicId: string | null;
  questions: Question[];
  myVotes: Set<string>;
  lastSeq: bigint | null;
  applyWelcome(snapshot: RoomSnapshot, seq: bigint): void;
  applyPresence(guests: Guest[], seq: bigint): void;
  applyTopicTree(topics: Topic[], activeTopicId: string | null, seq: bigint): void;
  applyQuestionAdded(question: Question, seq: bigint): void;
  applyQuestionUpdated(question: Question, seq: bigint): void;
  applyQuestionDeleted(questionId: string, seq: bigint): void;
  applyVoteUpdated(questionId: string, voteCount: number, voterGuestId: string, seq: bigint): void;
  setLastSeq(seq: bigint): void;
  reset(): void;
}

export const useSessionStore = create<SessionState>((set) => ({
  room: null,
  me: null,
  presence: [],
  topics: [],
  activeTopicId: null,
  questions: [],
  myVotes: new Set<string>(),
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
      topics: snapshot.topics,
      activeTopicId: snapshot.activeTopicId,
      questions: snapshot.questions,
      myVotes: new Set(snapshot.myVotes),
      lastSeq: seq,
    });
  },
  applyPresence(guests, seq) {
    set({ presence: guests, lastSeq: seq });
  },
  applyTopicTree(topics, activeTopicId, seq) {
    set({ topics, activeTopicId, lastSeq: seq });
  },
  applyQuestionAdded(question, seq) {
    set((state) => ({
      questions: [...state.questions, question],
      lastSeq: seq,
    }));
  },
  applyQuestionUpdated(question, seq) {
    set((state) => ({
      questions: state.questions.map((q) => q.id === question.id ? question : q),
      lastSeq: seq,
    }));
  },
  applyQuestionDeleted(questionId, seq) {
    set((state) => ({
      questions: state.questions.filter((q) => q.id !== questionId),
      lastSeq: seq,
    }));
  },
  applyVoteUpdated(questionId, voteCount, voterGuestId, seq) {
    set((state) => {
      const newMyVotes = new Set(state.myVotes);
      const isMyVote = voterGuestId === state.me?.guestId;
      if (isMyVote) {
        if (voteCount > newMyVotes.size) {
          newMyVotes.add(questionId);
        } else {
          newMyVotes.delete(questionId);
        }
      }
      return {
        questions: state.questions.map((q) =>
          q.id === questionId ? { ...q, voteCount } : q
        ),
        myVotes: newMyVotes,
        lastSeq: seq,
      };
    });
  },
  setLastSeq(seq) {
    set({ lastSeq: seq });
  },
  reset() {
    set({ room: null, me: null, presence: [], topics: [], activeTopicId: null, questions: [], myVotes: new Set(), lastSeq: null });
  },
}));
