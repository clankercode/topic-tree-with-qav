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
    default:
      store.setLastSeq(msg.seq);
  }
}
