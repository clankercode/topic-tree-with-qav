// G.1 — TopicTree must render the *whole* topic tree, not just root
// entries. Before this fix `TopicTree.tsx` filtered by
// `parent_id == null` and never recursed, so nested topics existed in
// the store but never appeared in the DOM.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, within } from "@testing-library/react";

import { TopicTree } from "../../src/components/TopicTree";
import { useSessionStore } from "../../src/store";
import type { Topic } from "../../src/ws/types";

vi.mock("../../src/ws/manager", () => ({
  sendWsMsg: vi.fn(),
}));

function topic(id: string, parentId: string | null, ord: number, title: string): Topic {
  return {
    id,
    parentId,
    title,
    ord,
    status: "pending",
  };
}

describe("TopicTree.G1 — recursive children", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("renders nested topics two levels deep", () => {
    useSessionStore.setState({
      topics: [
        topic("A", null, 1, "Root A"),
        topic("B", "A", 1, "Child of A"),
        topic("C", "B", 1, "Grandchild of A"),
        topic("D", null, 2, "Root D"),
      ],
      me: { clientId: "c1", role: "host", guestId: "g1" },
    });
    const { getByText, container } = render(<TopicTree />);

    // All four titles must appear in the DOM.
    expect(getByText("Root A")).toBeDefined();
    expect(getByText("Child of A")).toBeDefined();
    expect(getByText("Grandchild of A")).toBeDefined();
    expect(getByText("Root D")).toBeDefined();

    // Sanity: only two top-level <li> elements at the outer <ul> level.
    const outerList = container.querySelector("ul");
    expect(outerList).not.toBeNull();
    const directChildren = Array.from(outerList!.children).filter(
      (n) => n.tagName === "LI",
    );
    expect(directChildren).toHaveLength(2);

    // The first root <li> must contain a nested <ul> with B.
    const firstRoot = directChildren[0];
    const nestedUls = within(firstRoot as HTMLElement).getAllByRole("list");
    expect(nestedUls.length).toBeGreaterThanOrEqual(1);
    expect(within(nestedUls[0]).getByText("Child of A")).toBeDefined();
  });

  it("renders an empty placeholder when there are no topics at all", () => {
    useSessionStore.setState({
      topics: [],
      me: { clientId: "c1", role: "host", guestId: "g1" },
    });
    const { getByText } = render(<TopicTree />);
    expect(getByText(/No topics yet/)).toBeDefined();
  });

  it("guards against parent_id pointing at a missing topic", () => {
    useSessionStore.setState({
      topics: [
        topic("A", null, 1, "Root A"),
        // B's parent_id "GONE" references a topic that was deleted —
        // an inconsistent server snapshot we should not crash on.
        topic("B", "GONE", 1, "Orphan"),
      ],
      me: { clientId: "c1", role: "host", guestId: "g1" },
    });
    const { getByText, queryByText } = render(<TopicTree />);
    expect(getByText("Root A")).toBeDefined();
    // Orphan still renders at the top level (defensive fallback).
    expect(queryByText("Orphan")).not.toBeNull();
  });
});
