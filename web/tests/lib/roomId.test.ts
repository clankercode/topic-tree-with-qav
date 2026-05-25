import { describe, expect, it } from "vitest";
import { isValidRoomId } from "../../src/lib/roomId";

describe("isValidRoomId", () => {
  it("accepts 12-char base32 ids", () => {
    expect(isValidRoomId("ABCDEFGH2JKL")).toBe(true);
  });

  it("rejects lowercase ids", () => {
    expect(isValidRoomId("abcdefghijkl")).toBe(false);
  });

  it("rejects wrong length", () => {
    expect(isValidRoomId("ABC")).toBe(false);
  });
});
