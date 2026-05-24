import { useState } from "react";

interface AdminBannerProps {
  joinUrl: string;
  adminUrl: string;
}

export function AdminBanner({ joinUrl, adminUrl }: AdminBannerProps) {
  const [copied, setCopied] = useState<"join" | "admin" | null>(null);

  async function copy(kind: "join" | "admin", text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(kind);
      window.setTimeout(() => setCopied(null), 1500);
    } catch {
      setCopied(null);
    }
  }

  return (
    <div
      data-testid="admin-banner"
      className="rounded border border-[rgb(var(--border))] p-3 text-sm space-y-2"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono truncate" title={joinUrl}>
          {joinUrl}
        </span>
        <button
          type="button"
          onClick={() => copy("join", joinUrl)}
          className="px-2 py-1 rounded bg-[rgb(var(--surface))]"
        >
          {copied === "join" ? "Copied" : "Copy join URL"}
        </button>
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono truncate" title="(admin URL hidden)">
          (admin URL — keep private)
        </span>
        <button
          type="button"
          onClick={() => copy("admin", adminUrl)}
          className="px-2 py-1 rounded bg-[rgb(var(--surface))]"
        >
          {copied === "admin" ? "Copied" : "Copy admin URL"}
        </button>
      </div>
    </div>
  );
}
