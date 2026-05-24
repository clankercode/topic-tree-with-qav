import { useSessionStore } from "../store";

export function PresenceIndicator() {
  const presence = useSessionStore((s) => s.presence);
  const count = presence.length;
  return (
    <div
      data-testid="presence-indicator"
      aria-label={`${count} present`}
      className="text-sm text-[rgb(var(--muted))]"
    >
      {count} present
    </div>
  );
}
