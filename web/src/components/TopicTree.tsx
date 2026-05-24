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
import { useState } from "react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import { TopicNode } from "./TopicNode";
import { Plus } from "lucide-react";

export function TopicTree() {
  const { topics, activeTopicId, me } = useSessionStore();
  const [editingId, setEditingId] = useState<string | null>(null);
  const isHost = me?.role === "host";

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const rootTopics = topics
    .filter((t) => t.parentId === null)
    .sort((a, b) => a.ord - b.ord);

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id || !isHost) return;

    const oldIndex = rootTopics.findIndex((t) => t.id === active.id);
    const newIndex = rootTopics.findIndex((t) => t.id === over.id);
    if (oldIndex === -1 || newIndex === -1) return;

    const movedTopic = rootTopics[oldIndex];
    const afterTopic = newIndex > 0 ? rootTopics[newIndex - 1] : null;

    sendWsMsg({
      v: 1,
      type: "MoveTopic",
      topicId: movedTopic.id,
      newParentId: null,
      afterId: afterTopic?.id ?? null,
    });
  }

  function handleAddTopic() {
    if (!isHost) return;
    const title = prompt("Topic title:");
    if (!title?.trim()) return;
    sendWsMsg({
      v: 1,
      type: "AddTopic",
      parentId: null,
      title: title.trim(),
      afterId: rootTopics.length > 0 ? rootTopics[rootTopics.length - 1].id : null,
    });
  }

  return (
    <section className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-4">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-medium">Topics</h2>
        {isHost && (
          <button
            onClick={handleAddTopic}
            className="flex items-center gap-1 rounded bg-[rgb(var(--primary))] px-3 py-1 text-sm font-medium text-[rgb(var(--primary-fg))] hover:opacity-90"
          >
            <Plus size={16} />
            Add topic
          </button>
        )}
      </div>

      {rootTopics.length === 0 ? (
        <p className="text-sm text-[rgb(var(--muted))]">
          No topics yet. {isHost ? "Add one to get started." : "Waiting for host to add topics."}
        </p>
      ) : (
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
      )}
    </section>
  );
}
