import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, within } from "@testing-library/react";

import { RoomSessionTabs } from "../../src/components/RoomSessionTabs";
import { useSessionStore } from "../../src/store";

vi.mock("../../src/ws/manager", () => ({
  sendWsMsg: vi.fn(),
}));

describe("RoomSessionTabs", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
    useSessionStore.setState({
      me: { clientId: "c1", role: "host", guestId: "h1" },
      topics: [],
      boards: [],
    });
  });

  it("defaults to Topics & Q&A and hides the whiteboards panel", () => {
    const { getByTestId, getByText } = render(
      <RoomSessionTabs
        sortMode="chronological"
        onSortChange={() => {}}
        showHandsQueue={false}
      />,
    );

    expect(getByTestId("room-tab-topics")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(getByTestId("room-panel-topics")).not.toHaveAttribute("hidden");
    expect(getByTestId("room-panel-whiteboards")).toHaveAttribute("hidden");
    expect(getByText("Q&A")).toBeDefined();
  });

  it("shows the whiteboards panel when the Whiteboards tab is selected", () => {
    const { getByTestId, getByText } = render(
      <RoomSessionTabs
        sortMode="chronological"
        onSortChange={() => {}}
        showHandsQueue={false}
      />,
    );

    fireEvent.click(getByTestId("room-tab-whiteboards"));

    expect(getByTestId("room-tab-whiteboards")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(getByTestId("room-panel-whiteboards")).not.toHaveAttribute("hidden");
    expect(getByTestId("room-panel-topics")).toHaveAttribute("hidden");
    expect(getByText("No boards yet.")).toBeDefined();
  });

  it("renders HandsQueue only when showHandsQueue is true", () => {
    useSessionStore.setState({
      hands: [
        { guestId: "g1", displayName: "Amy", topic: "Question", raisedAt: 1 },
      ],
    });

    const withQueue = render(
      <RoomSessionTabs
        sortMode="chronological"
        onSortChange={() => {}}
        showHandsQueue={true}
      />,
    );
    expect(
      within(withQueue.getByTestId("room-panel-topics")).getByText("Question"),
    ).toBeDefined();

    withQueue.unmount();

    const withoutQueue = render(
      <RoomSessionTabs
        sortMode="chronological"
        onSortChange={() => {}}
        showHandsQueue={false}
      />,
    );
    expect(
      within(withoutQueue.getByTestId("room-panel-topics")).queryByText(
        "Question",
      ),
    ).toBeNull();
  });
});
