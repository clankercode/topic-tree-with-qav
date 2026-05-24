import { create } from "zustand";
import type { Board, Guest, PenBoardContent, PenText, Question, RoomSnapshot, RoomSummary, Topic } from "../ws/types";
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
  boards: Board[];
  focusedBoardId: string | null;
  penBoards: Map<string, PenBoardContent>;
  penInProgressStrokes: Map<string, { color: string; size: number; points: [number, number, number][] }>;
  lastSeq: bigint | null;
  applyWelcome(snapshot: RoomSnapshot, seq: bigint): void;
  applyPresence(guests: Guest[], seq: bigint): void;
  applyTopicTree(topics: Topic[], activeTopicId: string | null, seq: bigint): void;
  applyQuestionAdded(question: Question, seq: bigint): void;
  applyQuestionUpdated(question: Question, seq: bigint): void;
  applyQuestionDeleted(questionId: string, seq: bigint): void;
  applyVoteUpdated(questionId: string, voteCount: number, voterGuestId: string, seq: bigint): void;
  applyPenStrokeBegun(boardId: string, strokeId: string, color: string, size: number): void;
  applyPenStrokeAppended(boardId: string, strokeId: string, points: [number, number, number][]): void;
  applyPenStrokeEnded(boardId: string, strokeId: string): void;
  applyPenTextUpserted(boardId: string, text: PenText): void;
  applyPenTextDeleted(boardId: string, textId: string): void;
  applyPenCleared(boardId: string): void;
  applyPenUndone(boardId: string, removedStrokeId: string | null, removedTextId: string | null): void;
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
  boards: [],
  focusedBoardId: null,
  penBoards: new Map(),
  penInProgressStrokes: new Map(),
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
      focusedBoardId: snapshot.focusedBoardId,
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
  applyPenStrokeBegun(boardId, strokeId, color, size) {
    set((state) => {
      const newInProgress = new Map(state.penInProgressStrokes);
      newInProgress.set(`${boardId}:${strokeId}`, { color, size, points: [] });
      return { penInProgressStrokes: newInProgress };
    });
  },
  applyPenStrokeAppended(boardId, strokeId, points) {
    set((state) => {
      const key = `${boardId}:${strokeId}`;
      const existing = state.penInProgressStrokes.get(key);
      if (!existing) return state;
      const newInProgress = new Map(state.penInProgressStrokes);
      newInProgress.set(key, { ...existing, points: [...existing.points, ...points] });
      return { penInProgressStrokes: newInProgress };
    });
  },
  applyPenStrokeEnded(boardId, strokeId) {
    set((state) => {
      const key = `${boardId}:${strokeId}`;
      const stroke = state.penInProgressStrokes.get(key);
      if (!stroke) return state;
      const newInProgress = new Map(state.penInProgressStrokes);
      newInProgress.delete(key);
      const newPenBoards = new Map(state.penBoards);
      const board = newPenBoards.get(boardId) ?? { strokes: [], texts: [] };
      const newStroke = {
        id: strokeId,
        color: stroke.color,
        size: stroke.size,
        points: stroke.points,
        createdAt: Date.now(),
        ord: board.strokes.length,
      };
      newPenBoards.set(boardId, {
        ...board,
        strokes: [...board.strokes, newStroke],
      });
      return { penInProgressStrokes: newInProgress, penBoards: newPenBoards };
    });
  },
  applyPenTextUpserted(boardId, text) {
    set((state) => {
      const newPenBoards = new Map(state.penBoards);
      const board = newPenBoards.get(boardId) ?? { strokes: [], texts: [] };
      const existingIdx = board.texts.findIndex((t) => t.id === text.id);
      const newTexts = existingIdx >= 0
        ? board.texts.map((t, i) => i === existingIdx ? text : t)
        : [...board.texts, text];
      newPenBoards.set(boardId, { ...board, texts: newTexts });
      return { penBoards: newPenBoards };
    });
  },
  applyPenTextDeleted(boardId, textId) {
    set((state) => {
      const newPenBoards = new Map(state.penBoards);
      const board = newPenBoards.get(boardId);
      if (!board) return state;
      newPenBoards.set(boardId, {
        ...board,
        texts: board.texts.filter((t) => t.id !== textId),
      });
      return { penBoards: newPenBoards };
    });
  },
  applyPenCleared(boardId) {
    set((state) => {
      const newPenBoards = new Map(state.penBoards);
      newPenBoards.set(boardId, { strokes: [], texts: [] });
      return { penBoards: newPenBoards };
    });
  },
  applyPenUndone(boardId, removedStrokeId, removedTextId) {
    set((state) => {
      const newPenBoards = new Map(state.penBoards);
      const board = newPenBoards.get(boardId);
      if (!board) return state;
      if (removedStrokeId) {
        newPenBoards.set(boardId, {
          ...board,
          strokes: board.strokes.filter((s) => s.id !== removedStrokeId),
        });
      }
      if (removedTextId) {
        newPenBoards.set(boardId, {
          ...board,
          texts: board.texts.filter((t) => t.id !== removedTextId),
        });
      }
      return { penBoards: newPenBoards };
    });
  },
  setLastSeq(seq) {
    set({ lastSeq: seq });
  },
  reset() {
    set({
      room: null,
      me: null,
      presence: [],
      topics: [],
      activeTopicId: null,
      questions: [],
      myVotes: new Set(),
      boards: [],
      focusedBoardId: null,
      penBoards: new Map(),
      penInProgressStrokes: new Map(),
      lastSeq: null,
    });
  },
}));
