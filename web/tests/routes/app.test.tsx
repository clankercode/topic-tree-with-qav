import { describe, expect, it, beforeEach, vi } from "vitest";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import { AppRoutes } from "../../src/App";
import { __resetIdbForTests, getRoom } from "../../src/lib/idb";

function resetIdb() {
  globalThis.indexedDB = new IDBFactory();
  __resetIdbForTests();
}

const VALID_ROOM = "ABCDEFGH2JKL";

function mockRoomFetch(status: number, body?: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes(`/api/rooms/${VALID_ROOM}`)) {
        return new Response(JSON.stringify(body ?? {}), {
          status,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("not found", { status: 404 });
    }),
  );
}

describe("App routing", () => {
  beforeEach(() => {
    resetIdb();
    vi.unstubAllGlobals();
  });

  it("renders Landing with the create-room CTA at /", async () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      await screen.findByRole("heading", { level: 1, name: /topic-tree/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /create room/i }),
    ).toBeInTheDocument();
  });

  it("renders the About stub at /about", () => {
    render(
      <MemoryRouter initialEntries={["/about"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { name: /about/i })).toBeInTheDocument();
  });

  it("renders the Rooms dashboard at /rooms", () => {
    render(
      <MemoryRouter initialEntries={["/rooms"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      screen.getByRole("heading", { name: /your rooms/i }),
    ).toBeInTheDocument();
  });

  it("host claim stores the admin token and lands on the host shell", async () => {
    render(
      <MemoryRouter initialEntries={["/r/r1?admin=tok-xyz"]}>
        <AppRoutes />
      </MemoryRouter>,
    );

    await waitFor(async () => {
      const rec = await getRoom("r1");
      expect(rec?.adminToken).toBe("tok-xyz");
      expect(rec?.hostGuestId).toBeTruthy();
    });

    await waitFor(() => {
      expect(screen.getByTestId("host-shell")).toBeInTheDocument();
    });
    expect(
      screen.queryByText(/admin=tok-xyz/i, { selector: "*" }),
    ).not.toBeInTheDocument();
  });

  it("guest entry prompts for a display name at /r/:id", async () => {
    mockRoomFetch(200, { roomId: VALID_ROOM, title: "Demo room" });
    render(
      <MemoryRouter initialEntries={[`/r/${VALID_ROOM}`]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      await screen.findByRole("heading", { name: /join room/i }),
    ).toBeInTheDocument();
    expect(await screen.findByLabelText(/your name/i)).toBeInTheDocument();
  });

  it("redirects /r/ to home", async () => {
    render(
      <MemoryRouter initialEntries={["/r/"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      await screen.findByRole("heading", { level: 1, name: /topic-tree/i }),
    ).toBeInTheDocument();
  });

  it("renders not found for unknown routes", () => {
    render(
      <MemoryRouter initialEntries={["/nonsense"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      screen.getByRole("heading", { name: /page not found/i }),
    ).toBeInTheDocument();
  });

  it("preview entry prompts for a display name at /r/:id/preview", () => {
    render(
      <MemoryRouter initialEntries={["/r/r2/preview"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      screen.getByRole("heading", { name: /preview as guest/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/display name/i)).toBeInTheDocument();
  });
});
