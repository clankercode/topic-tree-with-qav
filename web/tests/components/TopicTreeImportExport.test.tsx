import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";

import { TopicTreeImportExport } from "../../src/components/TopicTreeImportExport";
import { useSessionStore } from "../../src/store";
import { useToastStore } from "../../src/store/toast";
import { resolvePendingSubmit, sendWsMsg } from "../../src/ws/manager";

vi.mock("../../src/ws/manager", async () => {
  const actual = await vi.importActual<typeof import("../../src/ws/manager")>(
    "../../src/ws/manager",
  );
  return {
    ...actual,
    sendWsMsg: vi.fn(),
  };
});

describe("TopicTreeImportExport", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
    useSessionStore.setState({
      me: { clientId: "host-client", role: "host", guestId: "host-guest" },
      room: { id: "room-1", title: "Room One", createdAt: 0 },
    });
    useToastStore.setState({ toasts: [] });
    vi.mocked(sendWsMsg).mockClear();
  });

  it("keeps the import modal and pasted JSON when the server rejects the import", () => {
    const pastedJson = JSON.stringify({
      version: 1,
      topics: [{ title: "Root", children: [] }],
    });

    render(<TopicTreeImportExport />);
    fireEvent.click(screen.getByRole("button", { name: "Import topic tree" }));

    const dialog = screen.getByRole("dialog");
    const textarea = within(dialog).getByRole("textbox") as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: pastedJson } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Import" }));

    const sent = vi.mocked(sendWsMsg).mock.calls[0][0];
    if (sent.type !== "ImportTopicTree") {
      throw new Error("expected ImportTopicTree");
    }
    expect(sent.id).toEqual(expect.any(String));

    act(() => {
      resolvePendingSubmit(sent.id!, {
        kind: "error",
        code: "bad_request",
        message: "imported tree is too deep",
      });
    });

    const openDialog = screen.getByRole("dialog");
    const openTextarea = within(openDialog).getByRole(
      "textbox",
    ) as HTMLTextAreaElement;
    expect(openTextarea.value).toBe(pastedJson);
    expect(within(openDialog).getByText("imported tree is too deep")).toBeDefined();
    expect(useToastStore.getState().toasts.at(-1)?.message).toContain(
      "imported tree is too deep",
    );
  });
});
