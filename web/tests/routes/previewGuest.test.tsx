import { beforeEach, describe, expect, it } from "vitest";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { PreviewGuestEntry } from "../../src/routes/PreviewGuestEntry";
import { __resetIdbForTests, getRoom, mergeRoomHost } from "../../src/lib/idb";
import { getPreviewGuest } from "../../src/lib/previewGuest";

function resetIdb() {
  globalThis.indexedDB = new IDBFactory();
  __resetIdbForTests();
}

describe("PreviewGuestEntry", () => {
  beforeEach(() => {
    resetIdb();
    sessionStorage.clear();
  });

  it("does not overwrite host credentials in IDB", async () => {
    await mergeRoomHost("r1", {
      adminToken: "tok-1",
      hostGuestId: "host-guest",
      title: "Demo",
      createdAt: 100,
      lastJoinedAt: 100,
    });

    render(
      <MemoryRouter initialEntries={["/r/r1/preview"]}>
        <Routes>
          <Route path="/r/:roomId/preview" element={<PreviewGuestEntry />} />
          <Route
            path="/r/:roomId/preview/guest"
            element={
              <div data-testid="preview-guest-route">preview session</div>
            }
          />
        </Routes>
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText(/display name/i), {
      target: { value: "Test Guest" },
    });
    fireEvent.click(screen.getByRole("button", { name: /start preview/i }));

    await waitFor(() => {
      expect(screen.getByTestId("preview-guest-route")).toBeInTheDocument();
    });

    const room = await getRoom("r1");
    expect(room?.adminToken).toBe("tok-1");
    expect(room?.hostGuestId).toBe("host-guest");
    expect(getPreviewGuest("r1")?.displayName).toBe("Test Guest");
  });
});
