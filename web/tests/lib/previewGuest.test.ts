import { beforeEach, describe, expect, it } from "vitest";
import {
  clearPreviewGuest,
  createPreviewGuestId,
  getPreviewGuest,
  savePreviewGuest,
} from "../../src/lib/previewGuest";

describe("previewGuest session storage", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it("save + get round-trips a preview record", () => {
    savePreviewGuest("r1", {
      guestId: "g-preview",
      displayName: "Preview Guest",
    });
    expect(getPreviewGuest("r1")).toEqual({
      guestId: "g-preview",
      displayName: "Preview Guest",
    });
  });

  it("createPreviewGuestId returns distinct ids", () => {
    const a = createPreviewGuestId();
    const b = createPreviewGuestId();
    expect(a).not.toBe(b);
    expect(a).toMatch(/^[0-9a-f-]{36}$/i);
  });

  it("clearPreviewGuest removes the record", () => {
    savePreviewGuest("r1", { guestId: "g1", displayName: "Test" });
    clearPreviewGuest("r1");
    expect(getPreviewGuest("r1")).toBeUndefined();
  });
});
