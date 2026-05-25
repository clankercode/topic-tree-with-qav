import { useEffect, useRef, useState } from "react";

interface ClickPing {
  id: string;
  x: number;
  y: number;
  displayName: string;
  timestamp: number;
}

interface ClickPingLayerProps {
  boardId: string;
  containerRef: React.RefObject<HTMLDivElement | null>;
}

export function ClickPingLayer({ boardId }: ClickPingLayerProps) {
  const [pings, setPings] = useState<ClickPing[]>([]);
  const pingsRef = useRef(pings);
  pingsRef.current = pings;

  useEffect(() => {
    const handleClicked = (e: Event) => {
      const customEvent = e as CustomEvent<{
        x: number;
        y: number;
        displayName: string;
      }>;
      const ping: ClickPing = {
        id: crypto.randomUUID(),
        x: customEvent.detail.x,
        y: customEvent.detail.y,
        displayName: customEvent.detail.displayName,
        timestamp: Date.now(),
      };
      setPings((prev) => [...prev, ping]);
      setTimeout(() => {
        setPings((prev) => prev.filter((p) => p.id !== ping.id));
      }, 1200);
    };
    const eventName = `click-ping-${boardId}` as keyof WindowEventMap;
    window.addEventListener(eventName, handleClicked);
    return () => window.removeEventListener(eventName, handleClicked);
  }, [boardId]);

  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      {pings.map((ping) => (
        <div
          key={ping.id}
          className="absolute"
          style={{
            left: ping.x,
            top: ping.y,
            transform: "translate(-50%, -50%)",
          }}
        >
          <div className="relative flex items-center justify-center">
            <div className="w-8 h-8 rounded-full border-2 border-[rgb(var(--click-ping-fill))] animate-ping opacity-75" />
            <div className="absolute -top-6 left-1/2 -translate-x-1/2 whitespace-nowrap rounded bg-[rgb(var(--click-ping-fill))] px-2 py-1 text-xs text-[rgb(var(--click-ping-fg))]">
              {ping.displayName}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
