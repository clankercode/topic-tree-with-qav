import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPenStrokeSampler, type PenPoint } from "../../src/lib/penStrokeSampler";

describe("penStrokeSampler", () => {
  let rafCallbacks: FrameRequestCallback[];
  let rafId: number;

  beforeEach(() => {
    rafCallbacks = [];
    rafId = 0;
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      rafCallbacks.push(cb);
      rafId += 1;
      return rafId;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {
      rafCallbacks = [];
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function flushOneFrame() {
    const cbs = [...rafCallbacks];
    rafCallbacks = [];
    for (const cb of cbs) cb(performance.now());
  }

  it("batches many pointer samples into frame flushes preserving order", () => {
    const sampler = createPenStrokeSampler();
    const batches: PenPoint[][] = [];

    sampler.start((points) => batches.push(points));

    for (let i = 0; i < 200; i += 1) {
      sampler.pushSample([i, i * 2, 0.5]);
    }

    // Simulate ~1s at 60fps: 60 frames
    for (let frame = 0; frame < 60; frame += 1) {
      flushOneFrame();
    }

    const remaining = sampler.stop();
    if (remaining.length > 0) batches.push(remaining);

    const allPoints = batches.flat();
    expect(batches.length).toBeLessThanOrEqual(62);
    expect(allPoints).toHaveLength(200);
    expect(allPoints[0]).toEqual([0, 0, 0.5]);
    expect(allPoints[199]).toEqual([199, 398, 0.5]);
  });

  it("does not flush empty frames", () => {
    const sampler = createPenStrokeSampler();
    const batches: PenPoint[][] = [];

    sampler.start((points) => batches.push(points));
    flushOneFrame();
    sampler.pushSample([1, 2, 0.5]);
    flushOneFrame();
    sampler.stop();

    expect(batches).toHaveLength(1);
    expect(batches[0]).toEqual([[1, 2, 0.5]]);
  });

  it("returns trailing samples on stop without scheduling another frame", () => {
    const sampler = createPenStrokeSampler();
    const batches: PenPoint[][] = [];

    sampler.start((points) => batches.push(points));
    sampler.pushSample([10, 20, 0.5]);
    sampler.pushSample([11, 21, 0.6]);

    const trailing = sampler.stop();
    flushOneFrame();

    expect(batches).toHaveLength(0);
    expect(trailing).toEqual([
      [10, 20, 0.5],
      [11, 21, 0.6],
    ]);
  });
});
