import { openDB, type IDBPDatabase } from "idb";
import { v4 as uuidv4 } from "uuid";

export type RoomRole = "admin" | "guest";

export interface RoomRecord {
  roomId: string;
  title: string;
  role: RoomRole;
  adminToken?: string;
  guestId: string;
  displayName?: string;
  createdAt: number;
  lastJoinedAt: number;
}

const DB_NAME = "tt-qav";
const DB_VERSION = 1;
const ROOMS_STORE = "rooms";
const META_STORE = "meta";
const GUEST_ID_KEY = "guestId";

let dbPromise: Promise<IDBPDatabase> | null = null;

function db(): Promise<IDBPDatabase> {
  if (!dbPromise) {
    dbPromise = openDB(DB_NAME, DB_VERSION, {
      upgrade(database) {
        if (!database.objectStoreNames.contains(ROOMS_STORE)) {
          database.createObjectStore(ROOMS_STORE, { keyPath: "roomId" });
        }
        if (!database.objectStoreNames.contains(META_STORE)) {
          database.createObjectStore(META_STORE);
        }
      },
      blocked() {
        dbPromise = null;
      },
      blocking() {
        dbPromise = null;
      },
      terminated() {
        dbPromise = null;
      },
    });
  }
  return dbPromise;
}

// Tests reset globalThis.indexedDB between cases; the cached db handle then
// points at the prior factory. Detect that and reopen.
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

export function __resetIdbForTests(): void {
  dbPromise = null;
}
