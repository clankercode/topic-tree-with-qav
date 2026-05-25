export const PEN_INK_PRIMARY = "#000000";
export const PEN_INK_INVERSE = "#ffffff";

export function resolvePenColor(color: string, isDark: boolean): string {
  if (color === PEN_INK_PRIMARY)
    return isDark ? PEN_INK_INVERSE : PEN_INK_PRIMARY;
  if (color === PEN_INK_INVERSE)
    return isDark ? PEN_INK_PRIMARY : PEN_INK_INVERSE;
  return color;
}

export function primaryInkForTheme(isDark: boolean): string {
  return isDark ? PEN_INK_INVERSE : PEN_INK_PRIMARY;
}

export function isAdaptiveInk(color: string): boolean {
  return color === PEN_INK_PRIMARY || color === PEN_INK_INVERSE;
}
