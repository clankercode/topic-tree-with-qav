import { openDB, type IDBPDatabase } from "idb";
import { v4 as uuidv4 } from "uuid";

export interface RoomGuestIdentity {
  guestId: string;
  displayName: string;
}

export interface RoomRecord {
  roomId: string;
  title: string;
  createdAt: number;
  lastJoinedAt: number;
  adminToken?: string;
  hostGuestId?: string;
  guest?: RoomGuestIdentity;
}

interface RoomRecordV1 {
  roomId: string;
  title: string;
  role: "admin" | "guest";
  adminToken?: string;
  guestId: string;
  displayName?: string;
  createdAt: number;
  lastJoinedAt: number;
}

const DB_NAME = "topic-tree-with-qav";
const DB_VERSION = 2;
const ROOMS_STORE = "rooms";
const META_STORE = "meta";
const GUEST_ID_KEY = "guestId";

let dbPromise: Promise<IDBPDatabase> | null = null;

function migrateV1Record(v1: RoomRecordV1): RoomRecord {
  const base = {
    roomId: v1.roomId,
    title: v1.title,
    createdAt: v1.createdAt,
    lastJoinedAt: v1.lastJoinedAt,
  };
  if (v1.role === "admin") {
    return {
      ...base,
      adminToken: v1.adminToken,
      hostGuestId: v1.guestId,
    };
  }
  return {
    ...base,
    guest: {
      guestId: v1.guestId,
      displayName: v1.displayName ?? "Guest",
    },
  };
}

function db(): Promise<IDBPDatabase> {
  if (!dbPromise) {
    dbPromise = openDB(DB_NAME, DB_VERSION, {
      async upgrade(database, oldVersion, _newVersion, transaction) {
        if (!database.objectStoreNames.contains(ROOMS_STORE)) {
          database.createObjectStore(ROOMS_STORE, { keyPath: "roomId" });
        }
        if (!database.objectStoreNames.contains(META_STORE)) {
          database.createObjectStore(META_STORE);
        }
        if (oldVersion > 0 && oldVersion < 2) {
          const store = transaction.objectStore(ROOMS_STORE);
          let cursor = await store.openCursor();
          while (cursor) {
            const value = cursor.value as RoomRecordV1 | RoomRecord;
            if ("role" in value && typeof value.role === "string") {
              await cursor.update(migrateV1Record(value as RoomRecordV1));
            }
            cursor = await cursor.continue();
          }
        }
      },
      blocked() {
        dbPromise = null;
        throw new Error(
          "Database blocked by another tab. Close other tabs using this app and retry.",
        );
      },
      blocking() {
        dbPromise = null;
      },
      terminated() {
        dbPromise = null;
      },
    }).catch((err) => {
      dbPromise = null;
      throw err;
    });
  }
  return dbPromise;
}

async function safeDb(): Promise<IDBPDatabase> {
  const handle = await db();
  try {
    void handle.objectStoreNames.length;
    return handle;
  } catch {
    dbPromise = null;
    return db();
  }
}

export async function upsertRoom(record: RoomRecord): Promise<void> {
  const handle = await safeDb();
  await handle.put(ROOMS_STORE, record);
}

export async function getRoom(roomId: string): Promise<RoomRecord | undefined> {
  const handle = await safeDb();
  return (await handle.get(ROOMS_STORE, roomId)) as RoomRecord | undefined;
}

export async function deleteRoom(roomId: string): Promise<void> {
  const handle = await safeDb();
  await handle.delete(ROOMS_STORE, roomId);
}

export async function listRooms(): Promise<RoomRecord[]> {
  const handle = await safeDb();
  const all = (await handle.getAll(ROOMS_STORE)) as RoomRecord[];
  return all.sort((a, b) => b.lastJoinedAt - a.lastJoinedAt);
}

export async function getOrCreateGuestId(): Promise<string> {
  const handle = await safeDb();
  const existing = (await handle.get(META_STORE, GUEST_ID_KEY)) as
    | string
    | undefined;
  if (existing) return existing;
  const fresh = uuidv4();
  await handle.put(META_STORE, fresh, GUEST_ID_KEY);
  return fresh;
}

export function roomHasHost(record: RoomRecord): boolean {
  return Boolean(record.adminToken);
}

export function roomHasGuest(record: RoomRecord): boolean {
  return Boolean(record.guest?.displayName);
}

export async function mergeRoomHost(
  roomId: string,
  patch: {
    adminToken?: string;
    hostGuestId?: string;
    title?: string;
    lastJoinedAt?: number;
    createdAt?: number;
  },
): Promise<RoomRecord> {
  const existing = await getRoom(roomId);
  const now = Date.now();
  const record: RoomRecord = {
    roomId,
    title: patch.title ?? existing?.title ?? "Untitled room",
    createdAt: patch.createdAt ?? existing?.createdAt ?? now,
    lastJoinedAt: patch.lastJoinedAt ?? now,
    adminToken: patch.adminToken ?? existing?.adminToken,
    hostGuestId: patch.hostGuestId ?? existing?.hostGuestId,
    guest: existing?.guest,
  };
  await upsertRoom(record);
  return record;
}

export async function mergeRoomGuest(
  roomId: string,
  patch: {
    guestId?: string;
    displayName?: string;
    title?: string;
    lastJoinedAt?: number;
    createdAt?: number;
  },
): Promise<RoomRecord> {
  const existing = await getRoom(roomId);
  const now = Date.now();
  const guestId = patch.guestId ?? existing?.guest?.guestId ?? uuidv4();
  const displayName = patch.displayName ?? existing?.guest?.displayName ?? "";
  const record: RoomRecord = {
    roomId,
    title: patch.title ?? existing?.title ?? "Untitled room",
    createdAt: patch.createdAt ?? existing?.createdAt ?? now,
    lastJoinedAt: patch.lastJoinedAt ?? now,
    adminToken: existing?.adminToken,
    hostGuestId: existing?.hostGuestId,
    guest: { guestId, displayName },
  };
  await upsertRoom(record);
  return record;
}

export async function getOrCreateRoomGuestId(roomId: string): Promise<string> {
  const existing = await getRoom(roomId);
  if (existing?.guest?.guestId) return existing.guest.guestId;
  const fresh = uuidv4();
  await mergeRoomGuest(roomId, {
    guestId: fresh,
    displayName: existing?.guest?.displayName ?? "",
  });
  return fresh;
}

export function __resetIdbForTests(): void {
  dbPromise = null;
}
