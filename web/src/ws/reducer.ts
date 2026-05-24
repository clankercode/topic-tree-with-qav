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
    case "TopicTreeUpdated":
      store.applyTopicTree(
        (msg as Extract<ServerMsg, { type: "TopicTreeUpdated" }>).topics,
        (msg as Extract<ServerMsg, { type: "TopicTreeUpdated" }>).activeTopicId,
        msg.seq,
      );
      return;
    case "QuestionAdded":
      store.applyQuestionAdded(
        (msg as Extract<ServerMsg, { type: "QuestionAdded" }>).question,
        msg.seq,
      );
      return;
    case "QuestionUpdated":
      store.applyQuestionUpdated(
        (msg as Extract<ServerMsg, { type: "QuestionUpdated" }>).question,
        msg.seq,
      );
      return;
    case "QuestionDeleted":
      store.applyQuestionDeleted(
        (msg as Extract<ServerMsg, { type: "QuestionDeleted" }>).questionId,
        msg.seq,
      );
      return;
    case "VoteUpdated": {
      const voteMsg = msg as Extract<ServerMsg, { type: "VoteUpdated" }>;
      store.applyVoteUpdated(
        voteMsg.questionId,
        voteMsg.voteCount,
        voteMsg.voterGuestId,
        msg.seq,
      );
      return;
    }
    case "PenStrokeBegun": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenStrokeBegun" }>;
      store.applyPenStrokeBegun(penMsg.boardId, penMsg.strokeId, penMsg.color, penMsg.size);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenStrokeAppended": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenStrokeAppended" }>;
      store.applyPenStrokeAppended(penMsg.boardId, penMsg.strokeId, penMsg.points);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenStrokeEnded": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenStrokeEnded" }>;
      store.applyPenStrokeEnded(penMsg.boardId, penMsg.strokeId);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenTextUpserted": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenTextUpserted" }>;
      store.applyPenTextUpserted(penMsg.boardId, penMsg.text);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenTextDeleted": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenTextDeleted" }>;
      store.applyPenTextDeleted(penMsg.boardId, penMsg.textId);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenCleared": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenCleared" }>;
      store.applyPenCleared(penMsg.boardId);
      store.setLastSeq(msg.seq);
      return;
    }
    case "PenUndone": {
      const penMsg = msg as Extract<ServerMsg, { type: "PenUndone" }>;
      store.applyPenUndone(penMsg.boardId, penMsg.removedStrokeId, penMsg.removedTextId);
      store.setLastSeq(msg.seq);
      return;
    }
    default:
      store.setLastSeq(msg.seq);
  }
}
