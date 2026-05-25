import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render } from "@testing-library/react";

import { PenBoard } from "../../src/components/PenBoard";
import { useSessionStore } from "../../src/store";
import { applyServerMessage } from "../../src/ws/reducer";
import { sendWsMsg } from "../../src/ws/manager";

vi.mock("../../src/ws/manager", async () => {
  const actual = await vi.importActual<typeof import("../../src/ws/manager")>(
    "../../src/ws/manager",
  );
  return {
    ...actual,
    sendWsMsg: vi.fn(),
  };
});

describe("PenBoard batched stroke appends", () => {
  let rafCallbacks: FrameRequestCallback[];

  beforeEach(() => {
    rafCallbacks = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      rafCallbacks.push(cb);
      return rafCallbacks.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {
      rafCallbacks = [];
    });
    useSessionStore.getState().reset();
    vi.mocked(sendWsMsg).mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function flushFrame() {
    const cbs = [...rafCallbacks];
    rafCallbacks = [];
    for (const cb of cbs) cb(performance.now());
  }

  it("sends batched PenStrokeAppend messages instead of one point per move", () => {
    render(
      <PenBoard
        boardId="b1"
        isHost
        content={{ strokes: [], texts: [] }}
      />,
    );

    const canvas = document.querySelector("canvas");
    expect(canvas).toBeTruthy();
    vi.spyOn(canvas!, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      width: 800,
      height: 450,
      top: 0,
      left: 0,
      right: 800,
      bottom: 450,
      toJSON: () => ({}),
    } as DOMRect);

    fireEvent.pointerDown(canvas!, {
      clientX: 100,
      clientY: 100,
      pressure: 0.5,
      pointerId: 1,
      bubbles: true,
    });

    for (let i = 0; i < 20; i += 1) {
      fireEvent.pointerMove(canvas!, {
        clientX: 100 + i,
        clientY: 100 + i,
        pressure: 0.5,
        pointerId: 1,
        bubbles: true,
      });
    }

    flushFrame();
    flushFrame();

    fireEvent.pointerUp(canvas!, { pointerId: 1, bubbles: true });

    const appendMsgs = vi
      .mocked(sendWsMsg)
      .mock.calls.map(([msg]) => msg)
      .filter((msg) => msg.type === "PenStrokeAppend") as unknown as Array<{
        type: "PenStrokeAppend";
        points: [number, number, number][];
      }>;

    expect(appendMsgs.length).toBeLessThan(20);
    const totalPoints = appendMsgs.reduce(
      (sum, msg) => sum + msg.points.length,
      0,
    );
    expect(totalPoints).toBeGreaterThanOrEqual(20);
  });
});

describe("PenBoard host echo suppression", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("keeps local in-progress point count when server echoes PenStrokeAppended", () => {
    useSessionStore.setState({
      me: { clientId: "c1", role: "host", guestId: "h1" },
      penInProgressStrokes: new Map([
        [
          "b1:s1",
          {
            color: "#000000",
            size: 4,
            points: [
              [10, 20, 0.5],
              [11, 21, 0.6],
            ],
          },
        ],
      ]),
    });

    act(() => {
      applyServerMessage({
        v: 1,
        ts: 0n,
        seq: 2n,
        type: "PenStrokeAppended",
        boardId: "b1",
        strokeId: "s1",
        points: [[30, 40, 0.7]],
      });
    });

    const stroke = useSessionStore.getState().penInProgressStrokes.get("b1:s1");
    expect(stroke?.points).toHaveLength(2);
  });
});
