import { useState } from "react";
import { BoardPanel } from "./BoardPanel";
import { HandsQueue } from "./HandsQueue";
import { QAPanel } from "./QAPanel";
import { TopicTree } from "./TopicTree";
import type { SortMode } from "./SortToggle";

type RoomTab = "topics" | "whiteboards";

type Props = {
  sortMode: SortMode;
  onSortChange: (mode: SortMode) => void;
  showHandsQueue: boolean;
};

const tabs: { id: RoomTab; label: string; testId: string }[] = [
  { id: "topics", label: "Topics & Q&A", testId: "room-tab-topics" },
  { id: "whiteboards", label: "Whiteboards", testId: "room-tab-whiteboards" },
];

export function RoomSessionTabs({
  sortMode,
  onSortChange,
  showHandsQueue,
}: Props) {
  const [activeTab, setActiveTab] = useState<RoomTab>("topics");

  return (
    <div className="space-y-4">
      <div
        role="tablist"
        aria-label="Room sections"
        className="flex items-center gap-1 border-b border-[rgb(var(--border))]"
      >
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            id={`${tab.id}-tab`}
            data-testid={tab.testId}
            aria-selected={activeTab === tab.id}
            aria-controls={`${tab.id}-panel`}
            onClick={() => setActiveTab(tab.id)}
            className={`px-4 py-2 text-sm font-medium whitespace-nowrap border-b-2 transition-colors ${
              activeTab === tab.id
                ? "border-[rgb(var(--accent))] text-[rgb(var(--accent))]"
                : "border-transparent text-[rgb(var(--muted))] hover:bg-[rgb(var(--muted))/10] hover:text-[rgb(var(--foreground))]"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div
        role="tabpanel"
        id="topics-panel"
        data-testid="room-panel-topics"
        aria-labelledby="topics-tab"
        hidden={activeTab !== "topics"}
        className={activeTab === "topics" ? undefined : "hidden"}
      >
        <div className="mx-auto max-w-5xl">
          <div className="grid gap-4 lg:grid-cols-2">
            <div className="flex flex-col gap-4">
              <TopicTree />
              {showHandsQueue ? (
                <section
                  aria-label="Raised hands queue"
                  className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-4"
                >
                  <HandsQueue />
                </section>
              ) : null}
            </div>
            <section className="flex max-h-[600px] min-h-[400px] flex-col rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))]">
              <QAPanel sortMode={sortMode} onSortChange={onSortChange} />
            </section>
          </div>
        </div>
      </div>

      <div
        role="tabpanel"
        id="whiteboards-panel"
        data-testid="room-panel-whiteboards"
        aria-labelledby="whiteboards-tab"
        hidden={activeTab !== "whiteboards"}
        className={activeTab !== "whiteboards" ? "hidden" : undefined}
      >
        <div className="mx-auto w-full max-w-7xl">
          <section className="flex min-h-[calc(100vh-14rem)] flex-col overflow-hidden rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))]">
            <BoardPanel />
          </section>
        </div>
      </div>
    </div>
  );
}
