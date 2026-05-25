import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { useMemo, useState } from "react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import { TopicNode } from "./TopicNode";
import { Plus } from "lucide-react";
import { AddTopicModal } from "./AddTopicModal";
import type { Topic } from "../ws/types";
import { TopicChildrenProvider, type ChildrenIndex } from "./TopicChildrenContext";

const ROOT_KEY = "__root__";

/// Group topics by `parentId`. Topics whose `parentId` points at a
/// missing topic are folded under the root so they remain visible (a
/// crash-resilient fallback for inconsistent snapshots).
function buildChildrenIndex(topics: Topic[]): ChildrenIndex {
  const known = new Set(topics.map((t) => t.id));
  const map = new Map<string, Topic[]>();
  for (const t of topics) {
    const key = t.parentId == null || !known.has(t.parentId) ? ROOT_KEY : t.parentId;
    const existing = map.get(key);
    if (existing) existing.push(t);
    else map.set(key, [t]);
  }
  for (const list of map.values()) list.sort((a, b) => a.ord - b.ord);
  return { map, rootKey: ROOT_KEY };
}

export function TopicTree() {
  const topics = useSessionStore((s) => s.topics);
  const activeTopicId = useSessionStore((s) => s.activeTopicId);
  const me = useSessionStore((s) => s.me);
  const optimisticAddTopic = useSessionStore((s) => s.optimisticAddTopic);
  const optimisticMoveTopic = useSessionStore((s) => s.optimisticMoveTopic);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);
  const isHost = me?.role === "host";

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const childrenIndex = useMemo(() => buildChildrenIndex(topics), [topics]);
  const rootTopics = childrenIndex.map.get(ROOT_KEY) ?? [];

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id || !isHost) return;

    const oldIndex = rootTopics.findIndex((t) => t.id === active.id);
    const newIndex = rootTopics.findIndex((t) => t.id === over.id);
    if (oldIndex === -1 || newIndex === -1) return;

    const movedTopic = rootTopics[oldIndex];
    const afterTopic = newIndex > 0 ? rootTopics[newIndex - 1] : null;

    optimisticMoveTopic(movedTopic.id, null, afterTopic?.id ?? null);
    sendWsMsg({
      v: 1,
      type: "MoveTopic",
      topicId: movedTopic.id,
      newParentId: null,
      afterId: afterTopic?.id ?? null,
    });
  }

  function handleAddTopic(title: string) {
    const tempId = crypto.randomUUID();
    const afterId =
      rootTopics.length > 0 ? rootTopics[rootTopics.length - 1].id : null;
    optimisticAddTopic(tempId, null, title, afterId);
    sendWsMsg({
      v: 1,
      type: "AddTopic",
      id: tempId,
      parentId: null,
      title,
      afterId,
    });
  }

  return (
    <section className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-4">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-medium">Topics</h2>
        {isHost && (
          <button
            onClick={() => setShowAddModal(true)}
            className="flex items-center gap-1 rounded bg-[rgb(var(--primary))] px-3 py-1 text-sm font-medium text-[rgb(var(--primary-fg))] hover:opacity-90"
          >
            <Plus size={16} />
            Add topic
          </button>
        )}
      </div>

      {rootTopics.length === 0 ? (
        <p className="text-sm text-[rgb(var(--muted))]">
          No topics yet.{" "}
          {isHost
            ? "Add one to get started."
            : "Waiting for host to add topics."}
        </p>
      ) : (
        <TopicChildrenProvider value={childrenIndex}>
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragEnd={handleDragEnd}
          >
            <SortableContext
              items={rootTopics.map((t) => t.id)}
              strategy={verticalListSortingStrategy}
            >
              <ul className="space-y-2">
                {rootTopics.map((topic) => (
                  <TopicNode
                    key={topic.id}
                    topic={topic}
                    isActive={activeTopicId === topic.id}
                    isEditing={editingId === topic.id}
                    onStartEdit={() => setEditingId(topic.id)}
                    onEndEdit={() => setEditingId(null)}
                  />
                ))}
              </ul>
            </SortableContext>
          </DndContext>
        </TopicChildrenProvider>
      )}
      <AddTopicModal
        open={showAddModal}
        onClose={() => setShowAddModal(false)}
        onAdd={handleAddTopic}
      />
    </section>
  );
}
