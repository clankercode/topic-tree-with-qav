import { beforeEach, describe, expect, it } from "vitest";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import {
  __resetIdbForTests,
  deleteRoom,
  getOrCreateGuestId,
  getRoom,
  listRooms,
  upsertRoom,
  type RoomRecord,
} from "../../src/lib/idb";

function resetIndexedDB() {
  globalThis.indexedDB = new IDBFactory();
  __resetIdbForTests();
}

function sampleAdmin(over: Partial<RoomRecord> = {}): RoomRecord {
  return {
    roomId: "r1",
    title: "Demo",
    role: "admin",
    adminToken: "tok-1",
    guestId: "guest-uuid-aaaa",
    displayName: undefined,
    createdAt: 1_700_000_000_000,
    lastJoinedAt: 1_700_000_000_000,
    ...over,
  };
}

describe("idb room registry", () => {
  beforeEach(() => {
    resetIndexedDB();
  });

  it("upsert + get round-trips a room record", async () => {
    const rec = sampleAdmin();
    await upsertRoom(rec);
    const got = await getRoom("r1");
    expect(got).toEqual(rec);
  });

  it("upsert overwrites existing record by roomId", async () => {
    await upsertRoom(sampleAdmin({ title: "First" }));
    await upsertRoom(sampleAdmin({ title: "Second", lastJoinedAt: 1_700_000_000_500 }));
    const got = await getRoom("r1");
    expect(got?.title).toBe("Second");
    expect(got?.lastJoinedAt).toBe(1_700_000_000_500);
  });

  it("getRoom returns undefined for unknown room", async () => {
    expect(await getRoom("missing")).toBeUndefined();
  });

  it("listRooms returns rooms sorted by lastJoinedAt desc", async () => {
    await upsertRoom(sampleAdmin({ roomId: "a", lastJoinedAt: 100 }));
    await upsertRoom(sampleAdmin({ roomId: "b", lastJoinedAt: 300 }));
    await upsertRoom(sampleAdmin({ roomId: "c", lastJoinedAt: 200 }));
    const rooms = await listRooms();
    expect(rooms.map((r) => r.roomId)).toEqual(["b", "c", "a"]);
  });

  it("deleteRoom removes the record", async () => {
    await upsertRoom(sampleAdmin());
    await deleteRoom("r1");
    expect(await getRoom("r1")).toBeUndefined();
    expect(await listRooms()).toHaveLength(0);
  });

  it("getOrCreateGuestId is stable across calls", async () => {
    const first = await getOrCreateGuestId();
    const second = await getOrCreateGuestId();
    expect(first).toBe(second);
    expect(first).toMatch(/^[0-9a-f-]{36}$/i);
  });

  it("getOrCreateGuestId issues a fresh id on a clean database", async () => {
    const first = await getOrCreateGuestId();
    resetIndexedDB();
    const second = await getOrCreateGuestId();
    expect(second).not.toBe(first);
  });
});
