import { useLayoutEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getOrCreateGuestId, upsertRoom } from "../lib/idb";

interface HostClaimProps {
  adminToken: string;
}

export function HostClaim({ adminToken }: HostClaimProps) {
  const { roomId } = useParams();
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);

  useLayoutEffect(() => {
    if (!roomId) {
      setError("Room id is missing");
      return;
    }

    window.history.replaceState(null, "", `/r/${roomId}`);

    let cancelled = false;
    const now = Date.now();
    void getOrCreateGuestId()
      .then((guestId) =>
        upsertRoom({
          roomId,
          title: "Untitled room",
          role: "admin",
          adminToken,
          guestId,
          createdAt: now,
          lastJoinedAt: now,
        }),
      )
      .then(() => {
        if (!cancelled) navigate(`/r/${roomId}/host`, { replace: true });
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to claim room");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [adminToken, navigate, roomId]);

  return (
    <main className="min-h-full flex items-center justify-center p-8">
      {error ? (
        <p role="alert" className="text-sm text-red-500">
          {error}
        </p>
      ) : (
        <p className="text-[rgb(var(--muted))]">Opening host view…</p>
      )}
    </main>
  );
}
