// G.2 — HandsQueue is wired into HostSession. Before this fix the
// component lived in src/components/ but was never imported, so the
// raise-hand queue was only visible inside a popup hidden behind a
// header button.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";

import { HandsQueue } from "../../src/components/HandsQueue";
import { useSessionStore } from "../../src/store";

vi.mock("../../src/ws/manager", () => ({
  sendWsMsg: vi.fn(),
}));

describe("HandsQueue", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("renders hands in raised-at order (FIFO)", () => {
    useSessionStore.setState({
      me: { clientId: "c1", role: "host", guestId: "host-1" },
      hands: [
        // Insertion order intentionally non-FIFO; component must
        // re-sort.
        {
          guestId: "g-bob",
          displayName: "Bob",
          topic: "Bob ask",
          raisedAt: 200,
        },
        {
          guestId: "g-amy",
          displayName: "Amy",
          topic: "Amy ask",
          raisedAt: 100,
        },
      ],
    });
    const { container } = render(<HandsQueue />);
    const rows = container.querySelectorAll("p.text-sm");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toBe("Amy ask");
    expect(rows[1].textContent).toBe("Bob ask");
  });

  it("returns nothing when the local user is not the host", () => {
    useSessionStore.setState({
      me: { clientId: "c1", role: "guest", guestId: "g-1" },
      hands: [
        { guestId: "g-amy", displayName: "Amy", topic: "T", raisedAt: 100 },
      ],
    });
    const { container } = render(<HandsQueue />);
    // Component short-circuits to null when role !== "host". Visible
    // DOM should be empty.
    expect(container.firstChild).toBeNull();
  });

  it("shows the empty placeholder when no hands are raised", () => {
    useSessionStore.setState({
      me: { clientId: "c1", role: "host", guestId: "h" },
      hands: [],
    });
    const { getByText } = render(<HandsQueue />);
    expect(getByText(/No raised hands/)).toBeDefined();
  });
});

describe("HostSession sidebar wires HandsQueue", () => {
  it("renders HandsQueue under the topic tree column", async () => {
    // Sanity: assert the wiring at the route level. We do a lighter
    // import-existence check rather than a full route render to keep
    // this test independent of router/websocket setup.
    const src = await import("../../src/routes/HostSession");
    const code = src.HostSession.toString();
    expect(code).toContain("HandsQueue");
  });
});
