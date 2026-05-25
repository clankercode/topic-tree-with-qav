// G.4 — parity tests for the raise-hand topic word counter. The
// server runs the identical cases in
// `server/src/validation.rs::tests`.

import { describe, expect, it } from "vitest";

import {
  countTopicWords,
  isValidRaiseHandTopic,
} from "../../src/lib/validation";

describe("countTopicWords", () => {
  it.each<[string, number, string]>([
    ["", 0, "empty"],
    ["   ", 0, "whitespace only"],
    ["foo", 1, "single word"],
    ["foo bar", 2, "two words, single space"],
    ["foo  bar", 2, "two words, double space"],
    ["foo\tbar", 2, "tab separator"],
    ["foo\nbar", 2, "newline separator"],
    ["foo bar", 2, "NBSP separator"],
    ["foo‍bar", 1, "zero-width joiner does NOT split"],
    ["foo-bar", 1, "hyphen is part of the word"],
    ["a b c d e f g h i j", 10, "exactly ten words allowed"],
    ["a b c d e f g h i j k", 11, "eleven words exceeds limit"],
  ])("counts %s as %d (%s)", (input, expected) => {
    expect(countTopicWords(input)).toBe(expected);
  });
});

describe("isValidRaiseHandTopic", () => {
  it("rejects whitespace-only", () => {
    expect(isValidRaiseHandTopic("   ")).toBe(false);
  });
  it("accepts up to 10 words", () => {
    expect(isValidRaiseHandTopic("a b c d e f g h i j")).toBe(true);
  });
  it("rejects 11 words", () => {
    expect(isValidRaiseHandTopic("a b c d e f g h i j k")).toBe(false);
  });
  it("rejects over 80 chars even if word-count is fine", () => {
    expect(isValidRaiseHandTopic("a".repeat(81))).toBe(false);
  });
});
