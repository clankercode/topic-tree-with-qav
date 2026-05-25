import { v4 as uuidv4 } from "uuid";

export interface PreviewGuestRecord {
  guestId: string;
  displayName: string;
}

function storageKey(roomId: string): string {
  return `previewGuest:${roomId}`;
}

export function createPreviewGuestId(): string {
  return uuidv4();
}

export function savePreviewGuest(
  roomId: string,
  record: PreviewGuestRecord,
): void {
  sessionStorage.setItem(storageKey(roomId), JSON.stringify(record));
}

export function getPreviewGuest(
  roomId: string,
): PreviewGuestRecord | undefined {
  const raw = sessionStorage.getItem(storageKey(roomId));
  if (!raw) return undefined;
  try {
    return JSON.parse(raw) as PreviewGuestRecord;
  } catch {
    return undefined;
  }
}

export function clearPreviewGuest(roomId: string): void {
  sessionStorage.removeItem(storageKey(roomId));
}
