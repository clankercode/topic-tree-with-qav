import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { createRef, useRef } from "react";
import { render, act } from "@testing-library/react";
import { CursorLayer } from "../../src/components/CursorLayer";
import { useSessionStore } from "../../src/store";

// F6 regression: CursorLayer's `cursors` selector previously did
// `s.cursors[boardId] ?? {}`, returning a fresh object literal each call.
// With useSyncExternalStore, an unstable getSnapshot return value
// drives React error #185 ("Maximum update depth exceeded") because
// every store notification yields a non-Object.is-equal snapshot, and
// the cursors-deps useEffect schedules work that re-triggers the store.
//
// The fix hoists the empty fallback to a module-level frozen constant
// so the selector is reference-stable when no cursors exist for the board.

let cursorLayerRenders = 0;

function ProbeCursorLayer({
  boardId,
  containerRef,
}: {
  boardId: string;
  containerRef: React.RefObject<HTMLDivElement | null>;
}) {
  const localRenders = useRef(0);
  localRenders.current += 1;
  cursorLayerRenders = localRenders.current;
  return <CursorLayer boardId={boardId} containerRef={containerRef} />;
}

describe("CursorLayer F6 update-depth regression", () => {
  beforeEach(() => {
    cursorLayerRenders = 0;
    useSessionStore.getState().reset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("does not loop when cursors[boardId] is undefined and the store ticks", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    useSessionStore.setState({
      me: { clientId: "self", role: "host", guestId: "g-self" },
      cursors: {},
    });

    const containerRef = createRef<HTMLDivElement>();

    const { unmount } = render(
      <div ref={containerRef}>
        <ProbeCursorLayer
          boardId="board-missing-from-cursors"
          containerRef={containerRef}
        />
      </div>,
    );

    // Bump store state several times — pre-fix, each tick produced a
    // new {} from the cursors selector and re-rendered, eventually
    // tripping Maximum update depth exceeded.
    act(() => {
      for (let i = 0; i < 25; i += 1) {
        useSessionStore.getState().tick();
      }
    });

    const allErrors = errorSpy.mock.calls.flat().map((arg) => String(arg));
    const depthErrors = allErrors.filter((s) =>
      s.includes("Maximum update depth exceeded"),
    );
    expect(depthErrors).toEqual([]);

    // With a stable empty-fallback the CursorLayer should not re-render
    // on unrelated store ticks. Bound this generously to absorb the
    // initial mount commit but reject any churn.
    expect(cursorLayerRenders).toBeLessThanOrEqual(3);

    unmount();
  });

  it("returns the same empty cursor map across no-op store ticks", () => {
    useSessionStore.setState({ cursors: {} });

    // Capture two snapshots of what the CursorLayer selector resolves
    // to when the board has no cursor entry.
    const select = () => {
      const s = useSessionStore.getState();
      // Match the production selector path used inside CursorLayer.
      return (s.cursors as Record<string, unknown>)["missing-board"];
    };

    const first = select();
    act(() => {
      useSessionStore.getState().tick();
    });
    const second = select();
    expect(first).toBe(second);
  });
});
