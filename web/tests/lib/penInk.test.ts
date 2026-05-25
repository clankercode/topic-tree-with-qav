import { describe, expect, it } from "vitest";
import {
  PEN_INK_INVERSE,
  PEN_INK_PRIMARY,
  primaryInkForTheme,
  resolvePenColor,
} from "../../src/lib/penInk";

describe("penInk", () => {
  it("primaryInkForTheme returns black in light mode and white in dark mode", () => {
    expect(primaryInkForTheme(false)).toBe(PEN_INK_PRIMARY);
    expect(primaryInkForTheme(true)).toBe(PEN_INK_INVERSE);
  });

  it("resolvePenColor maps primary ink to viewer theme", () => {
    expect(resolvePenColor(PEN_INK_PRIMARY, false)).toBe("#000000");
    expect(resolvePenColor(PEN_INK_PRIMARY, true)).toBe("#ffffff");
  });

  it("resolvePenColor maps inverse ink to viewer theme", () => {
    expect(resolvePenColor(PEN_INK_INVERSE, false)).toBe("#ffffff");
    expect(resolvePenColor(PEN_INK_INVERSE, true)).toBe("#000000");
  });

  it("resolvePenColor passes through chromatic colors unchanged", () => {
    expect(resolvePenColor("#ef4444", false)).toBe("#ef4444");
    expect(resolvePenColor("#ef4444", true)).toBe("#ef4444");
  });
});
