import { Link } from "react-router-dom";

export function NotFound() {
  return (
    <main className="min-h-full flex items-center justify-center p-8">
      <div className="max-w-sm space-y-4 text-center">
        <h1 className="text-2xl font-semibold tracking-tight">
          Page not found
        </h1>
        <p className="text-sm text-[rgb(var(--muted))]">
          That URL does not match any page in this app.
        </p>
        <Link
          to="/"
          className="inline-block rounded bg-[rgb(var(--accent))] px-4 py-2 text-white"
        >
          Go home
        </Link>
      </div>
    </main>
  );
}
