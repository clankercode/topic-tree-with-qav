import { useEffect, useRef } from "react";
import { useSessionStore, type CursorPosition } from "../store";

const EMPTY_CURSORS: Record<string, CursorPosition> = Object.freeze({}) as Record<string, CursorPosition>;

interface CursorProps {
  cursor: CursorPosition;
  targetX: number;
  targetY: number;
}

function Cursor({ cursor, targetX, targetY }: CursorProps) {
  return (
    <div
      className="pointer-events-none absolute flex flex-col items-start"
      style={{
        transform: `translate(${targetX}px, ${targetY}px)`,
      }}
    >
      <svg
        width="16"
        height="20"
        viewBox="0 0 16 20"
        fill="none"
        className="drop-shadow-md"
      >
        <path
          d="M0 0L0 16L4 12L7 19L9 18L6 11L12 11L0 0Z"
          fill="#3B82F6"
          stroke="white"
          strokeWidth="1"
        />
      </svg>
      <span
        className="ml-1 mt-0.5 whitespace-nowrap rounded-md bg-blue-500 px-1.5 py-0.5 text-xs font-medium text-white shadow-sm"
      >
        {cursor.displayName}
      </span>
    </div>
  );
}

interface CursorLayerProps {
  boardId: string;
  containerRef: React.RefObject<HTMLDivElement | null>;
  onMouseMove?: (x: number, y: number) => void;
  onMouseClick?: (x: number, y: number) => void;
}

export function CursorLayer({
  boardId,
  containerRef,
  onMouseMove,
  onMouseClick,
}: CursorLayerProps) {
  const cursors = useSessionStore((s) => s.cursors[boardId] ?? EMPTY_CURSORS);
  const me = useSessionStore((s) => s.me);
  const tick = useSessionStore((s) => s.tick);

  const positionsRef = useRef<Record<string, { current: { x: number; y: number }; target: { x: number; y: number } }>>({});
  const rafRef = useRef<number>(0);
  const lastTickRef = useRef<number>(0);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleMouseMove = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      onMouseMove?.(x, y);
    };

    const handleClick = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      onMouseClick?.(x, y);
    };

    container.addEventListener("mousemove", handleMouseMove);
    container.addEventListener("click", handleClick);
    return () => {
      container.removeEventListener("mousemove", handleMouseMove);
      container.removeEventListener("click", handleClick);
    };
  }, [containerRef, onMouseMove, onMouseClick]);

  useEffect(() => {
    const interpolate = () => {
      const now = Date.now();
      if (now - lastTickRef.current > 50) {
        tick();
        lastTickRef.current = now;
      }

      const positions = positionsRef.current;
      let needsUpdate = false;

      for (const [clientId, cursor] of Object.entries(cursors)) {
        const pos = positions[clientId] ?? { current: { x: cursor.x, y: cursor.y }, target: { x: cursor.x, y: cursor.y } };
        pos.target = { x: cursor.x, y: cursor.y };
        positions[clientId] = pos;
      }

      for (const clientId of Object.keys(positions)) {
        if (!cursors[clientId]) {
          delete positions[clientId];
          continue;
        }
        const pos = positions[clientId];
        const dx = pos.target.x - pos.current.x;
        const dy = pos.target.y - pos.current.y;
        if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
          pos.current.x += dx * 0.3;
          pos.current.y += dy * 0.3;
          needsUpdate = true;
        }
      }

      if (needsUpdate) {
        rafRef.current = requestAnimationFrame(interpolate);
      }
    };

    rafRef.current = requestAnimationFrame(interpolate);
    return () => {
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
    };
  }, [cursors, tick]);

  const positions = positionsRef.current;

  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      {Object.entries(cursors)
        .filter(([clientId]) => clientId !== me?.clientId)
        .map(([clientId, cursor]) => {
          const pos = positions[clientId] ?? { current: { x: cursor.x, y: cursor.y } };
          return (
            <Cursor
              key={clientId}
              cursor={cursor}
              targetX={pos.current.x}
              targetY={pos.current.y}
            />
          );
        })}
    </div>
  );
}
