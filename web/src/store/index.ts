import { create } from "zustand";
import type { Guest, Question, RoomSnapshot, RoomSummary, Topic } from "../ws/types";
import type { Role } from "../proto/generated";

export interface Me {
  clientId: string;
  role: Role;
  guestId: string;
}

type PendingOp =
  | { type: "add"; tempId: string; parentId: string | null; title: string; afterId: string | null }
  | { type: "rename"; topicId: string; title: string }
  | { type: "move"; topicId: string; newParentId: string | null; afterId: string | null }
  | { type: "delete"; topicId: string };

export interface SessionState {
  room: RoomSummary | null;
  me: Me | null;
  presence: Guest[];
  topics: Topic[];
  activeTopicId: string | null;
  questions: Question[];
  myVotes: Set<string>;
  lastSeq: bigint | null;
  pendingOps: Map<string, PendingOp>;
  applyWelcome(snapshot: RoomSnapshot, seq: bigint): void;
  applyPresence(guests: Guest[], seq: bigint): void;
  applyTopicTree(topics: Topic[], activeTopicId: string | null, seq: bigint): void;
  applyQuestionAdded(question: Question, seq: bigint): void;
  applyQuestionUpdated(question: Question, seq: bigint): void;
  applyQuestionDeleted(questionId: string, seq: bigint): void;
  applyVoteUpdated(questionId: string, voteCount: number, voterGuestId: string, seq: bigint): void;
  setLastSeq(seq: bigint): void;
  reset(): void;
  optimisticAddTopic(tempId: string, parentId: string | null, title: string, afterId: string | null): string;
  optimisticRenameTopic(topicId: string, title: string): void;
  optimisticMoveTopic(topicId: string, newParentId: string | null, afterId: string | null): void;
  optimisticDeleteTopic(topicId: string): Topic | null;
  clearPendingOp(tempId: string): void;
  clearPendingOpByTopic(topicId: string): void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  room: null,
  me: null,
  presence: [],
  topics: [],
  activeTopicId: null,
  questions: [],
  myVotes: new Set<string>(),
  lastSeq: null,
  pendingOps: new Map(),
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
      pendingOps: new Map(),
    });
  },
  applyPresence(guests, seq) {
    set({ presence: guests, lastSeq: seq });
  },
  applyTopicTree(topics, activeTopicId, seq) {
    set((state) => {
      const serverTopicIds = new Set(topics.map((t) => t.id));
      const finalTopics: Topic[] = [];
      const newPendingOps = new Map(state.pendingOps);

      for (const topic of topics) {
        const pending = state.pendingOps.get(topic.id);
        if (pending?.type === "delete") continue;
        if (pending?.type === "rename") {
          finalTopics.push({ ...topic, title: pending.title });
          newPendingOps.delete(topic.id);
        } else if (pending?.type === "move") {
          finalTopics.push({ ...topic, parentId: pending.newParentId });
          newPendingOps.delete(topic.id);
        } else {
          finalTopics.push(topic);
        }
      }

      for (const [id, op] of state.pendingOps) {
        if (op.type === "add") {
          if (serverTopicIds.has(id)) {
            newPendingOps.delete(id);
          } else {
            const matchedServerTopic = topics.find(
              (t) => t.title === op.title && t.parentId === op.parentId && t.id !== id
            );
            if (matchedServerTopic) {
              newPendingOps.delete(id);
            }
          }
        }
      }

      return { topics: finalTopics, activeTopicId, lastSeq: seq, pendingOps: newPendingOps };
    });
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
    set({ room: null, me: null, presence: [], topics: [], activeTopicId: null, questions: [], myVotes: new Set(), lastSeq: null, pendingOps: new Map() });
  },
  optimisticAddTopic(tempId, parentId, title, afterId) {
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.set(tempId, { type: "add", tempId, parentId, title, afterId });
      const maxOrd = state.topics
        .filter((t) => t.parentId === parentId)
        .reduce((max, t) => Math.max(max, t.ord), -1);
      const newTopic: Topic = {
        id: tempId,
        parentId,
        title,
        ord: maxOrd + 1,
        status: "pending",
        createdAt: Date.now(),
      };
      return {
        topics: [...state.topics, newTopic],
        pendingOps: newPendingOps,
      };
    });
    return tempId;
  },
  optimisticRenameTopic(topicId, title) {
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.set(topicId, { type: "rename", topicId, title });
      return {
        topics: state.topics.map((t) =>
          t.id === topicId ? { ...t, title } : t
        ),
        pendingOps: newPendingOps,
      };
    });
  },
  optimisticMoveTopic(topicId, newParentId, afterId) {
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.set(topicId, { type: "move", topicId, newParentId, afterId });
      return {
        topics: state.topics.map((t) =>
          t.id === topicId ? { ...t, parentId: newParentId } : t
        ),
        pendingOps: newPendingOps,
      };
    });
  },
  optimisticDeleteTopic(topicId) {
    const state = get();
    const topic = state.topics.find((t) => t.id === topicId);
    if (!topic) return null;
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.set(topicId, { type: "delete", topicId });
      const childIds = state.topics.filter((t) => t.parentId === topicId).map((t) => t.id);
      childIds.forEach((id) => {
        newPendingOps.set(id, { type: "delete", topicId: id });
      });
      return {
        topics: state.topics.filter((t) => t.id !== topicId && !childIds.includes(t.id)),
        pendingOps: newPendingOps,
      };
    });
    return topic;
  },
  clearPendingOp(tempId) {
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.delete(tempId);
      return { pendingOps: newPendingOps };
    });
  },
  clearPendingOpByTopic(topicId) {
    set((state) => {
      const newPendingOps = new Map(state.pendingOps);
      newPendingOps.delete(topicId);
      return { pendingOps: newPendingOps };
    });
  },
}));
