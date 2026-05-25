// G.5 — QuestionComposer must restore the user's draft when the
// server rejects the submission for `rate_limit` or `muted`. Without
// this fix the input was cleared immediately on submit, so a
// rejection meant the user had to retype.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";

import { QuestionComposer } from "../../src/components/QuestionComposer";
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

describe("QuestionComposer.G5 — draft rollback on error", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
    useSessionStore.setState({
      me: { clientId: "c1", role: "guest", guestId: "g-1" },
      connectionStatus: "connected",
    });
    useToastStore.setState({ toasts: [] });
    vi.mocked(sendWsMsg).mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("clears input on Ack matching the submission's refId", () => {
    render(<QuestionComposer />);
    const input = screen.getByPlaceholderText(
      "Ask a question...",
    ) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "Hello?" } });
    fireEvent.click(screen.getByRole("button", { name: /submit/i }));

    expect(input.value).toBe("");
    const sent = vi.mocked(sendWsMsg).mock.calls[0][0];
    if (sent.type !== "SubmitQuestion") {
      throw new Error("expected SubmitQuestion");
    }
    resolvePendingSubmit(sent.id!, { kind: "ack" });
    expect(input.value).toBe("");
  });

  it("restores input on Error{code:'rate_limit'}", () => {
    render(<QuestionComposer />);
    const input = screen.getByPlaceholderText(
      "Ask a question...",
    ) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "Spam?" } });
    fireEvent.click(screen.getByRole("button", { name: /submit/i }));
    expect(input.value).toBe("");

    const sent = vi.mocked(sendWsMsg).mock.calls[0][0];
    if (sent.type !== "SubmitQuestion") {
      throw new Error("expected SubmitQuestion");
    }
    act(() =>
      resolvePendingSubmit(sent.id!, {
        kind: "error",
        code: "rate_limit",
        message: "too fast",
      }),
    );
    expect(input.value).toBe("Spam?");
  });

  it("restores input on Error{code:'muted'}", () => {
    render(<QuestionComposer />);
    const input = screen.getByPlaceholderText(
      "Ask a question...",
    ) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "Question while muted" } });
    fireEvent.click(screen.getByRole("button", { name: /submit/i }));
    expect(input.value).toBe("");

    const sent = vi.mocked(sendWsMsg).mock.calls[0][0];
    if (sent.type !== "SubmitQuestion") {
      throw new Error("expected SubmitQuestion");
    }
    act(() =>
      resolvePendingSubmit(sent.id!, {
        kind: "error",
        code: "muted",
        message: "you are muted",
      }),
    );
    expect(input.value).toBe("Question while muted");
  });

  it("restores input and clears submitting when the submission times out", () => {
    vi.useFakeTimers();
    render(<QuestionComposer />);
    const input = screen.getByPlaceholderText(
      "Ask a question...",
    ) as HTMLTextAreaElement;
    const anonymous = screen.getByLabelText(
      /ask anonymously/i,
    ) as HTMLInputElement;

    fireEvent.change(input, { target: { value: "Slow question?" } });
    fireEvent.click(anonymous);
    fireEvent.click(screen.getByRole("button", { name: /submit/i }));

    expect(input.value).toBe("");
    expect(screen.getByRole("button", { name: /submit/i })).toBeDisabled();

    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(input.value).toBe("Slow question?");
    expect(anonymous.checked).toBe(true);
    expect(screen.getByRole("button", { name: /submit/i })).toBeEnabled();
    expect(useToastStore.getState().toasts.at(-1)?.message).toBe(
      "Submission timed out — please retry.",
    );
  });

  it("restores input when the connection drops before Ack or Error", () => {
    vi.useFakeTimers();
    render(<QuestionComposer />);
    const input = screen.getByPlaceholderText(
      "Ask a question...",
    ) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "Disconnected question?" } });
    fireEvent.click(screen.getByRole("button", { name: /submit/i }));

    expect(input.value).toBe("");

    act(() => {
      useSessionStore.setState({ connectionStatus: "disconnected" });
    });

    expect(input.value).toBe("Disconnected question?");
    expect(screen.getByRole("button", { name: /submit/i })).toBeEnabled();
    expect(useToastStore.getState().toasts.at(-1)?.message).toBe(
      "Submission timed out — please retry.",
    );
  });
});
