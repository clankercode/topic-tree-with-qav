import { getOrCreateGuestId, upsertRoom } from "./idb";

export interface CreatedRoom {
  roomId: string;
  title: string;
  adminToken: string;
  adminUrl: string;
  joinUrl: string;
}

interface CreateRoomRequest {
  title?: string;
}

export async function createRoom(req: CreateRoomRequest = {}): Promise<CreatedRoom> {
  const res = await fetch("/api/rooms", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) {
    throw new Error(`createRoom failed: HTTP ${res.status}`);
  }
  const body = (await res.json()) as Partial<CreatedRoom>;
  const roomId = body.roomId ?? (body as Partial<CreatedRoom> & { id?: string }).id;
  if (!roomId || !body.adminToken) {
    throw new Error("createRoom response missing roomId or adminToken");
  }
  const joinUrl = body.joinUrl ?? `${window.location.origin}/r/${roomId}`;
  return {
    roomId,
    title: body.title ?? "Untitled room",
    adminToken: body.adminToken,
    adminUrl: body.adminUrl ?? `${joinUrl}?admin=${encodeURIComponent(body.adminToken)}`,
    joinUrl,
  };
}

export async function persistCreatedRoomAsAdmin(room: CreatedRoom): Promise<void> {
  const guestId = await getOrCreateGuestId();
  const now = Date.now();
  await upsertRoom({
    roomId: room.roomId,
    title: room.title,
    role: "admin",
    adminToken: room.adminToken,
    guestId,
    createdAt: now,
    lastJoinedAt: now,
  });
}
