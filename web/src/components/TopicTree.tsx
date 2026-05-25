import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { useMemo, useState } from "react";
import { Plus } from "lucide-react";
import { useTopicCollapse } from "../hooks/useTopicCollapse";
import { siblingsOf, wouldExceedDepth } from "../lib/topicTreeHelpers";
import { useSessionStore } from "../store";
import { useToastStore } from "../store/toast";
import { sendWsMsg } from "../ws/manager";
import type { Topic } from "../ws/types";
import { AddTopicModal } from "./AddTopicModal";
import { TopicNode } from "./TopicNode";
import { TopicTreeImportExport } from "./TopicTreeImportExport";
import {
  TopicChildrenProvider,
  type ChildrenIndex,
} from "./TopicChildrenContext";

const ROOT_KEY = "__root__";

function buildChildrenIndex(topics: Topic[]): ChildrenIndex {
  const known = new Set(topics.map((t) => t.id));
  const map = new Map<string, Topic[]>();
  for (const t of topics) {
    const key =
      t.parentId == null || !known.has(t.parentId) ? ROOT_KEY : t.parentId;
    const existing = map.get(key);
    if (existing) existing.push(t);
    else map.set(key, [t]);
  }
  for (const list of map.values()) list.sort((a, b) => a.ord - b.ord);
  return { map, rootKey: ROOT_KEY };
}

function RootDropZone() {
  const { setNodeRef, isOver } = useDroppable({ id: "root-drop" });
  return (
    <div
      ref={setNodeRef}
      className={`min-h-1 ${isOver ? "rounded bg-[rgb(var(--primary))]/10" : ""}`}
      aria-hidden
    />
  );
}

interface SiblingListProps {
  parentKey: string;
  childrenIndex: ChildrenIndex;
  topics: Topic[];
  editingId: string | null;
  onStartEdit: (id: string) => void;
  onEndEdit: () => void;
}

function SiblingList({
  parentKey,
  childrenIndex,
  topics,
  editingId,
  onStartEdit,
  onEndEdit,
}: SiblingListProps) {
  const list = childrenIndex.map.get(parentKey) ?? [];

  return (
    <TopicChildrenProvider value={childrenIndex}>
      <SortableContext
        items={list.map((t) => t.id)}
        strategy={verticalListSortingStrategy}
      >
        <ul className="space-y-2">
          {list.map((topic) => (
            <TopicNode
              key={topic.id}
              topic={topic}
              allTopics={topics}
              childrenIndex={childrenIndex}
              isEditing={editingId === topic.id}
              onStartEdit={() => onStartEdit(topic.id)}
              onEndEdit={onEndEdit}
            />
          ))}
        </ul>
      </SortableContext>
    </TopicChildrenProvider>
  );
}

export function TopicTree() {
  const topics = useSessionStore((s) => s.topics);
  const me = useSessionStore((s) => s.me);
  const room = useSessionStore((s) => s.room);
  const optimisticAddTopic = useSessionStore((s) => s.optimisticAddTopic);
  const optimisticMoveTopic = useSessionStore((s) => s.optimisticMoveTopic);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const isHost = me?.role === "host";
  const collapse = useTopicCollapse(room?.id);
  const addToast = useToastStore((s) => s.addToast);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const childrenIndex = useMemo(() => buildChildrenIndex(topics), [topics]);
  const rootTopics = childrenIndex.map.get(ROOT_KEY) ?? [];
  const draggingTopic = draggingId
    ? topics.find((t) => t.id === draggingId)
    : null;

  function sendMove(
    topicId: string,
    newParentId: string | null,
    afterId: string | null,
  ) {
    optimisticMoveTopic(topicId, newParentId, afterId);
    sendWsMsg({
      v: 1,
      type: "MoveTopic",
      topicId,
      newParentId,
      afterId,
    });
  }

  function handleDragStart(event: DragStartEvent) {
    setDraggingId(String(event.active.id));
  }

  function handleDragEnd(event: DragEndEvent) {
    setDraggingId(null);
    const { active, over } = event;
    if (!over || !isHost) return;

    const activeId = String(active.id);
    const overId = String(over.id);
    if (activeId === overId) return;

    if (overId === "root-drop") {
      if (wouldExceedDepth(topics, activeId, null)) {
        addToast("Topic tree cannot exceed 10 levels deep.", "error");
        return;
      }
      sendMove(activeId, null, null);
      return;
    }

    if (overId.startsWith("child:")) {
      const newParentId = overId.slice("child:".length);
      if (newParentId === activeId) return;
      if (wouldExceedDepth(topics, activeId, newParentId)) {
        addToast("Topic tree cannot exceed 10 levels deep.", "error");
        return;
      }
      sendMove(activeId, newParentId, null);
      collapse.expand(newParentId);
      return;
    }

    const activeTopic = topics.find((t) => t.id === activeId);
    const overTopic = topics.find((t) => t.id === overId);
    if (!activeTopic || !overTopic) return;

    if (activeTopic.parentId !== overTopic.parentId) {
      if (wouldExceedDepth(topics, activeId, overTopic.parentId)) {
        addToast("Topic tree cannot exceed 10 levels deep.", "error");
        return;
      }
      sendMove(activeId, overTopic.parentId, overTopic.id);
      return;
    }

    const siblings = siblingsOf(topics, activeTopic.parentId);
    const oldIndex = siblings.findIndex((t) => t.id === activeId);
    const newIndex = siblings.findIndex((t) => t.id === overId);
    if (oldIndex === -1 || newIndex === -1 || oldIndex === newIndex) return;

    const afterId =
      newIndex > oldIndex
        ? overId
        : newIndex > 0
          ? siblings[newIndex - 1].id
          : null;
    sendMove(activeId, activeTopic.parentId, afterId);
  }

  function handleAddTopic(title: string, parentId: string | null = null) {
    const siblings = siblingsOf(topics, parentId);
    const tempId = crypto.randomUUID();
    const afterId =
      siblings.length > 0 ? siblings[siblings.length - 1].id : null;
    optimisticAddTopic(tempId, parentId, title, afterId);
    sendWsMsg({
      v: 1,
      type: "AddTopic",
      id: tempId,
      parentId,
      title,
      afterId,
    });
    if (parentId) collapse.expand(parentId);
  }

  return (
    <section className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-4">
      <div className="mb-4 flex items-center justify-between gap-2">
        <h2 className="text-lg font-medium">Topics</h2>
        {isHost && (
          <div className="flex items-center gap-2">
            <TopicTreeImportExport />
            <button
              onClick={() => setShowAddModal(true)}
              className="flex items-center gap-1 rounded bg-[rgb(var(--primary))] px-3 py-1 text-sm font-medium text-[rgb(var(--primary-fg))] hover:opacity-90"
            >
              <Plus size={16} />
              Add topic
            </button>
          </div>
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
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
        >
          <RootDropZone />
          <SiblingList
            parentKey={ROOT_KEY}
            childrenIndex={childrenIndex}
            topics={topics}
            editingId={editingId}
            onStartEdit={setEditingId}
            onEndEdit={() => setEditingId(null)}
          />
          <DragOverlay>
            {draggingTopic ? (
              <div className="rounded border border-[rgb(var(--primary))] bg-[rgb(var(--background))] px-3 py-2 text-sm shadow-lg">
                {draggingTopic.title}
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      )}
      <AddTopicModal
        open={showAddModal}
        onClose={() => setShowAddModal(false)}
        onAdd={(title) => handleAddTopic(title, null)}
      />
    </section>
  );
}

// Re-export for tests that import buildChildrenIndex indirectly via TopicTree
export { buildChildrenIndex };
