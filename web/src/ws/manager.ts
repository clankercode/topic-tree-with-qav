import type { ClientMsg } from "./types";

let clientRef: { send: (msg: ClientMsg) => void } | null = null;

export function setWsClient(client: { send: (msg: ClientMsg) => void } | null) {
  clientRef = client;
}

export function sendWsMsg(msg: ClientMsg) {
  clientRef?.send(msg);
}
