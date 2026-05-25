export type PenPoint = [number, number, number];

export interface PenStrokeSampler {
  start(onFlush: (points: PenPoint[]) => void): void;
  pushSample(point: PenPoint): void;
  stop(): PenPoint[];
}

export function createPenStrokeSampler(): PenStrokeSampler {
  let buffer: PenPoint[] = [];
  let onFlush: ((points: PenPoint[]) => void) | null = null;
  let rafId: number | null = null;
  let active = false;

  const scheduleFlush = () => {
    if (rafId !== null || !active) return;
    rafId = requestAnimationFrame(() => {
      rafId = null;
      if (!active || buffer.length === 0) {
        if (active) scheduleFlush();
        return;
      }
      const batch = buffer;
      buffer = [];
      onFlush?.(batch);
      if (active) scheduleFlush();
    });
  };

  return {
    start(flush) {
      onFlush = flush;
      active = true;
      scheduleFlush();
    },

    pushSample(point) {
      buffer.push(point);
    },

    stop() {
      active = false;
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      const trailing = buffer;
      buffer = [];
      onFlush = null;
      return trailing;
    },
  };
}
