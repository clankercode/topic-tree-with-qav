import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import { useEffect } from "react";

import { ExcalidrawBoard } from "../../src/components/ExcalidrawBoard";
import { useThemeStore } from "../../src/store/theme";

const mockUpdateScene = vi.fn();

vi.mock("@excalidraw/excalidraw", () => ({
  Excalidraw: ({
    excalidrawAPI,
    theme,
  }: {
    excalidrawAPI?: (api: { updateScene: typeof mockUpdateScene }) => void;
    theme: string;
  }) => {
    useEffect(() => {
      excalidrawAPI?.({ updateScene: mockUpdateScene });
    }, [excalidrawAPI]);
    return <div data-testid="excalidraw-mock" data-theme={theme} />;
  },
}));

const baseBoard = {
  id: "exc-1",
  kind: "excalidraw" as const,
  title: "Board",
  ord: 0,
  createdAt: 0,
  sceneVersion: 5,
  elements: [{ id: "el-1", type: "rectangle" }],
  appState: { theme: "light" },
};

describe("ExcalidrawBoard host echo + theme", () => {
  beforeEach(() => {
    useThemeStore.setState({ mode: "dark", resolvedTheme: "dark" });
    mockUpdateScene.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("skips updateScene when incoming sceneVersion is not newer than local versionRef", () => {
    const { rerender } = render(
      <ExcalidrawBoard isHost board={baseBoard} />,
    );

    mockUpdateScene.mockClear();

    rerender(<ExcalidrawBoard isHost board={baseBoard} />);

    expect(mockUpdateScene).not.toHaveBeenCalled();
  });

  it("applies updateScene once when sceneVersion advances", () => {
    const { rerender } = render(
      <ExcalidrawBoard
        isHost
        board={{
          ...baseBoard,
          sceneVersion: 1,
          elements: [],
          appState: {},
        }}
      />,
    );

    mockUpdateScene.mockClear();

    rerender(
      <ExcalidrawBoard
        isHost
        board={{
          ...baseBoard,
          sceneVersion: 2,
          elements: [{ id: "el-2", type: "rectangle" }],
          appState: { theme: "light", viewBackgroundColor: "#fff" },
        }}
      />,
    );

    expect(mockUpdateScene).toHaveBeenCalledTimes(1);
    expect(mockUpdateScene.mock.calls[0][0].appState.theme).toBe("dark");
  });

  it("updates Excalidraw theme when resolvedTheme changes mid-session", () => {
    render(
      <ExcalidrawBoard
        isHost
        board={{
          ...baseBoard,
          sceneVersion: 1,
          elements: [],
          appState: {},
        }}
      />,
    );

    mockUpdateScene.mockClear();

    act(() => {
      useThemeStore.getState().setMode("light");
    });

    expect(mockUpdateScene).toHaveBeenCalledWith({
      appState: { theme: "light" },
    });
  });
});
