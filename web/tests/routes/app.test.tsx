import { describe, expect, it, beforeEach } from "vitest";
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

describe("App routing", () => {
  beforeEach(() => {
    resetIdb();
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
      expect(rec?.role).toBe("admin");
      expect(rec?.adminToken).toBe("tok-xyz");
    });

    await waitFor(() => {
      expect(screen.getByTestId("host-shell")).toBeInTheDocument();
    });
    expect(
      screen.queryByText(/admin=tok-xyz/i, { selector: "*" }),
    ).not.toBeInTheDocument();
  });

  it("guest entry prompts for a display name at /r/:id", () => {
    render(
      <MemoryRouter initialEntries={["/r/r2"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    expect(
      screen.getByRole("heading", { name: /join room/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/your name/i)).toBeInTheDocument();
  });
});
