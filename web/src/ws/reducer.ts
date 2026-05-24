import { useSessionStore } from "../store";
import type { ServerMsg } from "./types";

export function applyServerMessage(msg: ServerMsg): void {
  const store = useSessionStore.getState();
  switch (msg.type) {
    case "Welcome":
      store.applyWelcome(
        (msg as Extract<ServerMsg, { type: "Welcome" }>).snapshot,
        msg.seq,
      );
      return;
    case "PresenceUpdate":
      store.applyPresence(
        (msg as Extract<ServerMsg, { type: "PresenceUpdate" }>).guests,
        msg.seq,
      );
      return;
    case "TopicTreeUpdated": {
      const m = msg as Extract<ServerMsg, { type: "TopicTreeUpdated" }>;
      store.applyTopicTree(m.topics, m.activeTopicId, msg.seq);
      return;
    }
    case "QuestionAdded": {
      const m = msg as Extract<ServerMsg, { type: "QuestionAdded" }>;
      store.applyQuestionAdded(m.question, msg.seq);
      return;
    }
    case "QuestionUpdated": {
      const m = msg as Extract<ServerMsg, { type: "QuestionUpdated" }>;
      store.applyQuestionUpdated(m.question, msg.seq);
      return;
    }
    case "QuestionDeleted": {
      const m = msg as Extract<ServerMsg, { type: "QuestionDeleted" }>;
      store.applyQuestionDeleted(m.questionId, msg.seq);
      return;
    }
    case "VoteUpdated": {
      const m = msg as Extract<ServerMsg, { type: "VoteUpdated" }>;
      store.applyVoteUpdated(m.questionId, m.voteCount, m.voterGuestId, msg.seq);
      return;
    }
    case "BoardCreated": {
      const m = msg as Extract<ServerMsg, { type: "BoardCreated" }>;
      store.applyBoardCreated(m.board, msg.seq);
      return;
    }
    case "BoardUpdated": {
      const m = msg as Extract<ServerMsg, { type: "BoardUpdated" }>;
      store.applyBoardUpdated(m.board, msg.seq);
      return;
    }
    case "BoardDeleted": {
      const m = msg as Extract<ServerMsg, { type: "BoardDeleted" }>;
      store.applyBoardDeleted(m.boardId, msg.seq);
      return;
    }
    case "FocusedBoardChanged": {
      const m = msg as Extract<ServerMsg, { type: "FocusedBoardChanged" }>;
      store.applyFocusedBoardChanged(m.boardId, msg.seq);
      return;
    }
    case "ExcalidrawDelta": {
      const m = msg as Extract<ServerMsg, { type: "ExcalidrawDelta" }>;
      store.applyExcalidrawDelta(m.boardId, m.sceneVersion, m.elements, m.appState, msg.seq);
      return;
    }
    case "ExcalidrawSceneReset": {
      const m = msg as Extract<ServerMsg, { type: "ExcalidrawSceneReset" }>;
      store.applyExcalidrawSceneReset(m.boardId, m.sceneVersion, m.elements, m.appState, msg.seq);
      return;
    }
    case "PenStrokeBegun": {
      const m = msg as Extract<ServerMsg, { type: "PenStrokeBegun" }>;
      store.applyPenStrokeBegun(m.boardId, m.strokeId, m.color, m.size);
      return;
    }
    case "PenStrokeAppended": {
      const m = msg as Extract<ServerMsg, { type: "PenStrokeAppended" }>;
      store.applyPenStrokeAppended(m.boardId, m.strokeId, m.points);
      return;
    }
    case "PenStrokeEnded": {
      const m = msg as Extract<ServerMsg, { type: "PenStrokeEnded" }>;
      store.applyPenStrokeEnded(m.boardId, m.strokeId);
      return;
    }
    case "PenTextUpserted": {
      const m = msg as Extract<ServerMsg, { type: "PenTextUpserted" }>;
      store.applyPenTextUpserted(m.boardId, m.text);
      return;
    }
    case "PenTextDeleted": {
      const m = msg as Extract<ServerMsg, { type: "PenTextDeleted" }>;
      store.applyPenTextDeleted(m.boardId, m.textId);
      return;
    }
    case "PenCleared": {
      const m = msg as Extract<ServerMsg, { type: "PenCleared" }>;
      store.applyPenCleared(m.boardId);
      return;
    }
    case "PenUndone": {
      const m = msg as Extract<ServerMsg, { type: "PenUndone" }>;
      store.applyPenUndone(m.boardId, m.removedStrokeId, m.removedTextId);
      return;
    }
    default:
      store.setLastSeq(msg.seq);
  }
}
