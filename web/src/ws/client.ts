import type { Role } from "../proto/generated";
import { applyServerMessage } from "./reducer";
import type { ClientMsg, ServerMsg } from "./types";

export interface WebSocketLike {
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent<string>) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
}

export interface WsClientOptions {
  url: string;
  hello: {
    role: Role;
    guestId: string;
    displayName?: string;
    adminToken?: string;
  };
  socketFactory?: (url: string) => WebSocketLike;
  onOpen?: () => void;
  onClose?: () => void;
  onError?: (err: unknown) => void;
}

const BACKOFFS_MS = [1000, 2000, 4000, 8000, 16000, 30000];

function defaultFactory(url: string): WebSocketLike {
  return new WebSocket(url) as unknown as WebSocketLike;
}

export class WsClient {
  private readonly opts: WsClientOptions;
  private socket: WebSocketLike | null = null;
  private attempt = 0;
  private lastSeq: bigint | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;

  constructor(opts: WsClientOptions) {
    this.opts = opts;
  }

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket) {
      this.socket.onopen = null;
      this.socket.onmessage = null;
      this.socket.onclose = null;
      this.socket.onerror = null;
      try {
        this.socket.close(1000, "client stop");
      } catch (err) {
        this.opts.onError?.(err);
      }
      this.socket = null;
    }
  }

  send(msg: ClientMsg): void {
    if (!this.socket) return;
    this.socket.send(JSON.stringify(msg));
  }

  getLastSeq(): bigint | null {
    return this.lastSeq;
  }

  private connect(): void {
    const factory = this.opts.socketFactory ?? defaultFactory;
    const socket = factory(this.opts.url);
    this.socket = socket;
    socket.onopen = () => this.handleOpen();
    socket.onmessage = (ev) => this.handleMessage(ev);
    socket.onclose = () => this.handleClose();
    socket.onerror = (ev) => this.opts.onError?.(ev);
  }

  private handleOpen(): void {
    this.send({
      v: 1,
      type: "Hello",
      role: this.opts.hello.role,
      guestId: this.opts.hello.guestId,
      ...(this.opts.hello.displayName !== undefined
        ? { displayName: this.opts.hello.displayName }
        : {}),
      ...(this.opts.hello.adminToken !== undefined
        ? { adminToken: this.opts.hello.adminToken }
        : {}),
    });
    this.opts.onOpen?.();
  }

  private handleMessage(ev: MessageEvent<string>): void {
    let parsed: ServerMsg;
    try {
      parsed = parseEnvelope(ev.data);
    } catch (err) {
      this.opts.onError?.(err);
      return;
    }
    this.checkSeq(parsed.seq);
    if (parsed.type === "Ping") {
      this.send({ v: 1, type: "Pong" });
      return;
    }
    if (parsed.type === "Welcome") {
      this.attempt = 0;
    }
    applyServerMessage(parsed);
  }

  private checkSeq(seq: bigint): void {
    if (this.lastSeq !== null && seq !== this.lastSeq + 1n) {
      this.send({ v: 1, type: "GetSnapshot" });
    }
    this.lastSeq = seq;
  }

  private handleClose(): void {
    this.socket = null;
    this.opts.onClose?.();
    if (this.stopped) return;
    const delay = BACKOFFS_MS[Math.min(this.attempt, BACKOFFS_MS.length - 1)];
    this.attempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.stopped) this.connect();
    }, delay);
  }
}

function parseEnvelope(text: string): ServerMsg {
  const raw = JSON.parse(text, (_key, value) => value) as Record<
    string,
    unknown
  >;
  const seq = toBigInt(raw.seq);
  const ts = toBigInt(raw.ts);
  return { ...raw, seq, ts } as unknown as ServerMsg;
}

function toBigInt(value: unknown): bigint {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") return BigInt(value);
  if (typeof value === "string") return BigInt(value);
  throw new Error(`unexpected non-numeric envelope field: ${typeof value}`);
}
