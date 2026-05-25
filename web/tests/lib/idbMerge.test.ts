import { beforeEach, describe, expect, it } from "vitest";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import {
  __resetIdbForTests,
  getRoom,
  mergeRoomGuest,
  mergeRoomHost,
} from "../../src/lib/idb";

function resetIndexedDB() {
  globalThis.indexedDB = new IDBFactory();
  __resetIdbForTests();
}

describe("guest join after host claim", () => {
  beforeEach(() => {
    resetIndexedDB();
  });

  it("preserves adminToken when guest joins the same room", async () => {
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

    const room = await getRoom("r1");
    expect(room?.adminToken).toBe("tok-1");
    expect(room?.hostGuestId).toBe("host-guest");
    expect(room?.guest).toEqual({
      guestId: "room-guest",
      displayName: "Alice",
    });
  });
});
