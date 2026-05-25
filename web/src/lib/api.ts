import { getOrCreateGuestId, mergeRoomHost } from "./idb";

const DEFAULT_FETCH_TIMEOUT_MS = 15_000;

export interface CreatedRoom {
  roomId: string;
  title: string;
  adminToken: string;
  adminUrl: string;
  joinUrl: string;
}

export interface RoomSummary {
  roomId: string;
  title: string;
}

interface CreateRoomRequest {
  title?: string;
}

async function fetchWithTimeout(
  input: RequestInfo | URL,
  init: RequestInit = {},
  timeoutMs = DEFAULT_FETCH_TIMEOUT_MS,
): Promise<Response> {
  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(input, { ...init, signal: controller.signal });
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") {
      throw new Error("Server unreachable — is the backend running?");
    }
    throw err;
  } finally {
    window.clearTimeout(timer);
  }
}

export async function createRoom(
  req: CreateRoomRequest = {},
): Promise<CreatedRoom> {
  const res = await fetchWithTimeout("/api/rooms", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) {
    let msg = `createRoom failed: HTTP ${res.status}`;
    try {
      const err = await res.json();
      if (err.error) msg = err.error;
    } catch {
      // ignore parse error
    }
    throw new Error(msg);
  }
  const body = (await res.json()) as Partial<CreatedRoom>;
  const roomId =
    body.roomId ?? (body as Partial<CreatedRoom> & { id?: string }).id;
  if (!roomId || !body.adminToken) {
    throw new Error("createRoom response missing roomId or adminToken");
  }
  const joinUrl = body.joinUrl ?? `${window.location.origin}/r/${roomId}`;
  return {
    roomId,
    title: body.title ?? "Untitled room",
    adminToken: body.adminToken,
    adminUrl:
      body.adminUrl ??
      `${joinUrl}?admin=${encodeURIComponent(body.adminToken)}`,
    joinUrl,
  };
}

export async function fetchRoom(roomId: string): Promise<RoomSummary> {
  const res = await fetchWithTimeout(
    `/api/rooms/${encodeURIComponent(roomId)}`,
  );
  if (res.status === 404) {
    throw new Error("Room not found");
  }
  if (!res.ok) {
    let msg = `fetchRoom failed: HTTP ${res.status}`;
    try {
      const err = (await res.json()) as { error?: string };
      if (err.error) msg = err.error;
    } catch {
      // ignore parse error
    }
    throw new Error(msg);
  }
  const body = (await res.json()) as Partial<RoomSummary>;
  if (!body.roomId) {
    throw new Error("fetchRoom response missing roomId");
  }
  return {
    roomId: body.roomId,
    title: body.title ?? "Untitled room",
  };
}

export async function persistCreatedRoomAsAdmin(
  room: CreatedRoom,
): Promise<void> {
  const guestId = await getOrCreateGuestId();
  const now = Date.now();
  await mergeRoomHost(room.roomId, {
    title: room.title,
    adminToken: room.adminToken,
    hostGuestId: guestId,
    createdAt: now,
    lastJoinedAt: now,
  });
}
