export const ROOM_ID_LEN = 12;

export function isValidRoomId(id: string): boolean {
  if (id.length !== ROOM_ID_LEN) return false;
  for (let i = 0; i < id.length; i++) {
    const c = id.charCodeAt(i);
    const isUpper = c >= 65 && c <= 90;
    const isDigit = c >= 50 && c <= 55;
    if (!isUpper && !isDigit) return false;
  }
  return true;
}
