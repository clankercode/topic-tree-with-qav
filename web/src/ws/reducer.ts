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
    default:
      store.setLastSeq(msg.seq);
  }
}
