import { Link } from "react-router-dom";
import type { SessionError } from "../store";

interface SessionErrorViewProps {
  error: SessionError;
  roomId?: string;
}

const COPY: Record<
  Exclude<SessionError, null>,
  { title: string; message: string }
> = {
  room_not_found: {
    title: "Room not found",
    message:
      "This room does not exist or may have been removed. Check the link with the host.",
  },
  invalid_room: {
    title: "Invalid room link",
    message: "The room id in this URL is not valid.",
  },
  unauthorized: {
    title: "Access denied",
    message: "You do not have permission to open this room as host.",
  },
};

export function SessionErrorView({ error, roomId }: SessionErrorViewProps) {
  if (!error) return null;
  const { title, message } = COPY[error];
  return (
    <main className="min-h-full flex items-center justify-center p-8">
      <div className="max-w-sm space-y-4 text-center">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        <p className="text-sm text-[rgb(var(--muted))]">{message}</p>
        <div className="flex flex-col gap-2">
          <Link
            to="/"
            className="inline-block rounded bg-[rgb(var(--accent))] px-4 py-2 text-white"
          >
            Go home
          </Link>
          {roomId && error === "room_not_found" ? (
            <Link
              to={`/r/${roomId}`}
              className="text-sm text-[rgb(var(--accent))] underline"
            >
              Try joining again
            </Link>
          ) : null}
        </div>
      </div>
    </main>
  );
}
