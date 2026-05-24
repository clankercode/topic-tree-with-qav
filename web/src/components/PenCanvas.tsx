import { useEffect, useRef, useCallback } from "react";
import { getStroke } from "perfect-freehand";
import type { PenStrokeSummary } from "../ws/types";
import { useThemeStore } from "../store/theme";

const CANVAS_WIDTH = 4096;
const CANVAS_HEIGHT = 2304;

function strokeToPath(stroke: PenStrokeSummary): string {
  const points = stroke.points.map(([x, y, p]) => [x, y, p] as [number, number, number]);
  const outlinePoints = getStroke(points, {
    size: stroke.size,
    thinning: 0.5,
    smoothing: 0.5,
    streamline: 0.5,
    simulatePressure: points.length === 0 || (points.length === 1 && points[0][2] === 0),
  });

  if (outlinePoints.length === 0) return "";

  const d: string[] = [];
  for (let i = 0; i < outlinePoints.length; i++) {
    const [x, y] = outlinePoints[i];
    if (i === 0) {
      d.push(`M ${x} ${y}`);
    } else {
      d.push(`L ${x} ${y}`);
    }
  }
  d.push("Z");
  return d.join(" ");
}

interface PenCanvasProps {
  strokes: PenStrokeSummary[];
  inProgressStrokes: Map<string, { color: string; size: number; points: [number, number, number][] }>;
  onStrokeBegin?: (boardId: string, strokeId: string, x: number, y: number, pressure: number) => void;
  onStrokeAppend?: (boardId: string, strokeId: string, x: number, y: number, pressure: number) => void;
  onStrokeEnd?: (boardId: string, strokeId: string) => void;
  isHost?: boolean;
  boardId: string;
}

export function PenCanvas({
  strokes,
  inProgressStrokes,
  onStrokeBegin,
  onStrokeAppend,
  onStrokeEnd,
  isHost = false,
  boardId,
}: PenCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const currentStrokeRef = useRef<string | null>(null);

  const resolvedTheme = useThemeStore((s) => s.resolvedTheme);
  const isDark = resolvedTheme === "dark";

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    ctx.fillStyle = isDark ? "#1a1a1a" : "#ffffff";
    ctx.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);

    for (const stroke of strokes) {
      const pathData = strokeToPath(stroke);
      if (!pathData) continue;
      const path = new Path2D(pathData);
      ctx.fillStyle = stroke.color;
      ctx.fill(path);
    }

    inProgressStrokes.forEach((stroke) => {
      if (stroke.points.length === 0) return;
      const outlinePoints = getStroke(stroke.points, {
        size: stroke.size,
        thinning: 0.5,
        smoothing: 0.5,
        streamline: 0.5,
        simulatePressure: false,
      });
      if (outlinePoints.length === 0) return;
      const d: string[] = [];
      for (let i = 0; i < outlinePoints.length; i++) {
        const [x, y] = outlinePoints[i];
        if (i === 0) {
          d.push(`M ${x} ${y}`);
        } else {
          d.push(`L ${x} ${y}`);
        }
      }
      d.push("Z");
      const pathData = d.join(" ");
      const path = new Path2D(pathData);
      ctx.fillStyle = stroke.color;
      ctx.fill(path);
    });
  }, [strokes, inProgressStrokes, isDark]);

  useEffect(() => {
    draw();
  }, [draw]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const resizeObserver = new ResizeObserver(() => {
      draw();
    });
    resizeObserver.observe(container);
    return () => resizeObserver.disconnect();
  }, [draw]);

  const getCanvasPoint = (e: React.PointerEvent<HTMLCanvasElement>): { x: number; y: number; pressure: number } => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0, pressure: 0.5 };
    const rect = canvas.getBoundingClientRect();
    const scaleX = CANVAS_WIDTH / rect.width;
    const scaleY = CANVAS_HEIGHT / rect.height;
    const x = (e.clientX - rect.left) * scaleX;
    const y = (e.clientY - rect.top) * scaleY;
    const pressure = e.pressure || 0.5;
    return { x, y, pressure };
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isHost || !onStrokeBegin) return;
    e.preventDefault();
    const { x, y, pressure } = getCanvasPoint(e);
    const strokeId = crypto.randomUUID();
    currentStrokeRef.current = strokeId;
    onStrokeBegin(boardId, strokeId, x, y, pressure);
    (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isHost || !onStrokeAppend || !currentStrokeRef.current) return;
    const { x, y, pressure } = getCanvasPoint(e);
    const strokeId = currentStrokeRef.current;
    onStrokeAppend(boardId, strokeId, x, y, pressure);
  };

  const handlePointerUp = () => {
    if (!isHost || !onStrokeEnd) return;
    if (currentStrokeRef.current) {
      const strokeId = currentStrokeRef.current;
      onStrokeEnd(boardId, strokeId);
      currentStrokeRef.current = null;
    }
  };

  return (
    <div ref={containerRef} className="relative w-full overflow-hidden rounded" style={{ aspectRatio: "16/9" }}>
      <canvas
        ref={canvasRef}
        width={CANVAS_WIDTH}
        height={CANVAS_HEIGHT}
        className="w-full h-full"
        style={{ touchAction: "none" }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      />
    </div>
  );
}
