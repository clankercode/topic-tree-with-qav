import type { WsClient } from "./client";
import type { ClientMsg } from "./types";

let clientRef: WsClient | null = null;

export function setWsClient(client: WsClient | null) {
  clientRef = client;
}

export function stopWsClient(): void {
  clientRef?.stop();
  clientRef = null;
}

export function sendWsMsg(msg: ClientMsg) {
  clientRef?.send(msg);
}

/// Per-refId one-shot callbacks for intents whose UI needs to react
/// to the server's Ack or Error. The composer for Q&A submission
/// preserves its draft until the matching Ack lands so the user
/// doesn't lose typed text on rate_limit / muted rejection.
///
/// The map is process-local; a navigation or store reset clears it
/// because the component effect's cleanup unregisters.
export type SubmitOutcome =
  | { kind: "ack" }
  | { kind: "error"; code: string; message: string };

const pendingSubmits = new Map<string, (outcome: SubmitOutcome) => void>();

export function registerPendingSubmit(
  refId: string,
  callback: (outcome: SubmitOutcome) => void,
): () => void {
  pendingSubmits.set(refId, callback);
  return () => {
    pendingSubmits.delete(refId);
  };
}

export function resolvePendingSubmit(refId: string, outcome: SubmitOutcome) {
  const cb = pendingSubmits.get(refId);
  if (!cb) return;
  pendingSubmits.delete(refId);
  cb(outcome);
}
