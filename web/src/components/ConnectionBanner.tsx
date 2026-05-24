import { WifiOff, Loader2 } from "lucide-react";
import { useSessionStore } from "../store";

export function ConnectionBanner() {
  const connectionStatus = useSessionStore((s) => s.connectionStatus);

  if (connectionStatus === "connected") {
    return null;
  }

  if (connectionStatus === "connecting") {
    return (
      <div className="fixed top-0 left-0 right-0 z-50 flex items-center justify-center gap-2 bg-[rgb(var(--accent))] px-4 py-2 text-sm text-white">
        <Loader2 className="h-4 w-4 animate-spin" />
        <span>Connecting...</span>
      </div>
    );
  }

  return (
    <div className="fixed top-0 left-0 right-0 z-50 flex items-center justify-center gap-2 bg-red-600 px-4 py-2 text-sm text-white">
      <WifiOff className="h-4 w-4" />
      <span>Connection lost. Reconnecting...</span>
    </div>
  );
}
