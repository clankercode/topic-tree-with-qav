import { useSessionStore } from "../store";
import { useFollowHostStore } from "../store/followHost";
import { useToastStore } from "../store/toast";
import type { ServerMsg } from "./types";

export function applyServerMessage(msg: ServerMsg): void {
  const store = useSessionStore.getState();
  const followHostStore = useFollowHostStore.getState();
  const toastStore = useToastStore.getState();
  switch (msg.type) {
    case "Welcome":
      store.applyWelcome(
        (msg as Extract<ServerMsg, { type: "Welcome" }>).snapshot,
        msg.seq,
      );
      return;
    case "RoomSnapshot":
      store.applyWelcome(
        (msg as Extract<ServerMsg, { type: "RoomSnapshot" }>).snapshot,
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
      const isHost = store.me?.role === "host";
      if (isHost || followHostStore.followingHost) {
        store.applyFocusedBoardChanged(m.boardId, msg.seq);
      } else {
        store.setLastSeq(msg.seq);
      }
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
    case "CursorMoved": {
      const m = msg as Extract<ServerMsg, { type: "CursorMoved" }>;
      store.applyCursorMoved(m.boardId, m.clientId, m.guestId, m.displayName, m.x, m.y);
      return;
    }
    case "Clicked": {
      const m = msg as Extract<ServerMsg, { type: "Clicked" }>;
      store.setLastSeq(msg.seq);
      window.dispatchEvent(new CustomEvent(`click-ping-${m.boardId}`, {
        detail: { x: m.x, y: m.y, displayName: m.displayName },
      }));
      return;
    }
    case "HandsUpdated": {
      const m = msg as Extract<ServerMsg, { type: "HandsUpdated" }>;
      store.applyHandsUpdated(m.hands, msg.seq);
      return;
    }
    case "QuestionPromotedToTopic": {
      const m = msg as Extract<ServerMsg, { type: "QuestionPromotedToTopic" }>;
      store.applyQuestionDeleted(m.questionId, msg.seq);
      store.applyTopicTree([...store.topics, m.topic], store.activeTopicId, msg.seq);
      return;
    }
    case "KickNotice": {
      const m = msg as Extract<ServerMsg, { type: "KickNotice" }>;
      if (store.me?.guestId !== m.guestId) return;
      toastStore.addToast("You have been removed from this room.", "error");
      store.setKicked();
      return;
    }
    case "Error": {
      const m = msg as Extract<ServerMsg, { type: "Error" }>;
      if (m.code === "muted") {
        toastStore.addToast(m.message, "error");
      } else if (m.code === "rate_limit") {
        toastStore.addToast("Too many requests. Please slow down.", "error");
      } else if (m.code === "unauthorized" && m.message.includes("removed")) {
        store.setKicked();
      }
      store.setLastSeq(msg.seq);
      return;
    }
    default:
      store.setLastSeq(msg.seq);
  }
}
