import { beforeEach, describe, expect, it } from "vitest";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import {
  __resetIdbForTests,
  deleteRoom,
  getOrCreateGuestId,
  getOrCreateRoomGuestId,
  getRoom,
  listRooms,
  mergeRoomGuest,
  mergeRoomHost,
  upsertRoom,
  type RoomRecord,
} from "../../src/lib/idb";

function resetIndexedDB() {
  globalThis.indexedDB = new IDBFactory();
  __resetIdbForTests();
}

function sampleHost(over: Partial<RoomRecord> = {}): RoomRecord {
  return {
    roomId: "r1",
    title: "Demo",
    adminToken: "tok-1",
    hostGuestId: "guest-uuid-aaaa",
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
    const rec = sampleHost();
    await upsertRoom(rec);
    const got = await getRoom("r1");
    expect(got).toEqual(rec);
  });

  it("upsert overwrites existing record by roomId", async () => {
    await upsertRoom(sampleHost({ title: "First" }));
    await upsertRoom(
      sampleHost({ title: "Second", lastJoinedAt: 1_700_000_000_500 }),
    );
    const got = await getRoom("r1");
    expect(got?.title).toBe("Second");
    expect(got?.lastJoinedAt).toBe(1_700_000_000_500);
  });

  it("getRoom returns undefined for unknown room", async () => {
    expect(await getRoom("missing")).toBeUndefined();
  });

  it("listRooms returns rooms sorted by lastJoinedAt desc", async () => {
    await upsertRoom(sampleHost({ roomId: "a", lastJoinedAt: 100 }));
    await upsertRoom(sampleHost({ roomId: "b", lastJoinedAt: 300 }));
    await upsertRoom(sampleHost({ roomId: "c", lastJoinedAt: 200 }));
    const rooms = await listRooms();
    expect(rooms.map((r) => r.roomId)).toEqual(["b", "c", "a"]);
  });

  it("deleteRoom removes the record", async () => {
    await upsertRoom(sampleHost());
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

  it("mergeRoomGuest preserves host credentials", async () => {
    await mergeRoomHost("r1", {
      adminToken: "tok-1",
      hostGuestId: "host-guest",
      title: "Demo",
      createdAt: 100,
      lastJoinedAt: 100,
    });
    await mergeRoomGuest("r1", {
      displayName: "Alice",
      guestId: "room-guest",
      lastJoinedAt: 200,
    });
    const got = await getRoom("r1");
    expect(got?.adminToken).toBe("tok-1");
    expect(got?.hostGuestId).toBe("host-guest");
    expect(got?.guest).toEqual({
      guestId: "room-guest",
      displayName: "Alice",
    });
  });

  it("mergeRoomHost preserves guest credentials", async () => {
    await mergeRoomGuest("r1", {
      displayName: "Alice",
      guestId: "room-guest",
      createdAt: 100,
      lastJoinedAt: 100,
    });
    await mergeRoomHost("r1", {
      adminToken: "tok-1",
      hostGuestId: "host-guest",
      lastJoinedAt: 200,
    });
    const got = await getRoom("r1");
    expect(got?.adminToken).toBe("tok-1");
    expect(got?.guest).toEqual({
      guestId: "room-guest",
      displayName: "Alice",
    });
  });

  it("getOrCreateRoomGuestId is stable per room", async () => {
    const first = await getOrCreateRoomGuestId("r1");
    const second = await getOrCreateRoomGuestId("r1");
    expect(first).toBe(second);
    expect(first).toMatch(/^[0-9a-f-]{36}$/i);
  });

  it("getOrCreateRoomGuestId issues distinct ids per room", async () => {
    const a = await getOrCreateRoomGuestId("r1");
    const b = await getOrCreateRoomGuestId("r2");
    expect(a).not.toBe(b);
  });
});
