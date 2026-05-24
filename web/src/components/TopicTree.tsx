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
import { Plus, List, GitBranch } from "lucide-react";
import type { Topic } from "../ws/types";
import { cn } from "../lib/utils";

function generateTempId(): string {
  return `temp-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function sortTopics(topics: Topic[], parentId: string | null): Topic[] {
  return topics
    .filter((t) => t.parentId === parentId)
    .sort((a, b) => a.ord - b.ord);
}

interface TreeNodeProps {
  topic: Topic;
  topics: Topic[];
  activeTopicId: string | null;
  editingId: string | null;
  onStartEdit: (id: string) => void;
  onEndEdit: () => void;
  depth: number;
  isHost: boolean;
  onAddChild: (parentId: string) => void;
}

function TreeNode({
  topic,
  topics,
  activeTopicId,
  editingId,
  onStartEdit,
  onEndEdit,
  depth,
  isHost,
  onAddChild,
}: TreeNodeProps & { onAddChild: (parentId: string) => void }) {
  const [expanded, setExpanded] = useState(true);
  const children = sortTopics(topics, topic.id);
  const optimisticDeleteTopic = useSessionStore((s) => s.optimisticDeleteTopic);

  function handleDelete() {
    if (!isHost) return;
    if (!confirm(`Delete "${topic.title}" and all its subtopics?`)) return;
    optimisticDeleteTopic(topic.id);
    sendWsMsg({ v: 1, type: "DeleteTopic", topicId: topic.id });
  }

  return (
    <li className="relative">
      <div
        className={cn(
          "mb-1 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-3 py-2",
          activeTopicId === topic.id && "border-[rgb(var(--primary))] ring-2 ring-[rgb(var(--primary))]",
        )}
        style={{ marginLeft: depth * 20 }}
      >
        <TopicNode
          topic={topic}
          isActive={activeTopicId === topic.id}
          isEditing={editingId === topic.id}
          onStartEdit={() => onStartEdit(topic.id)}
          onEndEdit={onEndEdit}
          onDelete={handleDelete}
          showAddChild={isHost}
          onAddChild={() => onAddChild(topic.id)}
          hasChildren={children.length > 0}
          isExpanded={expanded}
          onToggleExpand={() => setExpanded(!expanded)}
        />
      </div>
      {children.length > 0 && expanded && (
        <ul className="relative border-l border-[rgb(var(--border))]" style={{ marginLeft: depth * 20 + 10 }}>
          {children.map((child) => (
            <TreeNode
              key={child.id}
              topic={child}
              topics={topics}
              activeTopicId={activeTopicId}
              editingId={editingId}
              onStartEdit={onStartEdit}
              onEndEdit={onEndEdit}
              depth={depth + 1}
              isHost={isHost}
              onAddChild={onAddChild}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export function TopicTree() {
  const { topics, activeTopicId, me, optimisticAddTopic } = useSessionStore();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"tree" | "list">("tree");
  const isHost = me?.role === "host";

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const rootTopics = sortTopics(topics, null);

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id || !isHost) return;

    const siblings = sortTopics(topics, null);
    const oldIndex = siblings.findIndex((t) => t.id === active.id);
    const newIndex = siblings.findIndex((t) => t.id === over.id);
    if (oldIndex === -1 || newIndex === -1) return;

    const movedTopic = siblings[oldIndex];
    const afterTopic = newIndex > 0 ? siblings[newIndex - 1] : null;

    useSessionStore.getState().optimisticMoveTopic(movedTopic.id, null, afterTopic?.id ?? null);
    sendWsMsg({
      v: 1,
      type: "MoveTopic",
      topicId: movedTopic.id,
      newParentId: null,
      afterId: afterTopic?.id ?? null,
    });
  }

  function handleAddTopic(parentId: string | null = null) {
    if (!isHost) return;
    const title = prompt("Topic title:");
    if (!title?.trim()) return;
    const tempId = generateTempId();
    const siblings = sortTopics(topics, parentId);
    optimisticAddTopic(tempId, parentId, title.trim(), siblings.length > 0 ? siblings[siblings.length - 1].id : null);
    sendWsMsg({
      v: 1,
      type: "AddTopic",
      parentId,
      title: title.trim(),
      afterId: siblings.length > 0 ? siblings[siblings.length - 1].id : null,
    });
  }

  return (
    <section className="rounded border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-4">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-medium">Topics</h2>
        <div className="flex items-center gap-2">
          <div className="flex rounded border border-[rgb(var(--border))]">
            <button
              onClick={() => setViewMode("tree")}
              className={cn(
                "flex items-center gap-1 px-2 py-1 text-xs",
                viewMode === "tree" ? "bg-[rgb(var(--primary))] text-[rgb(var(--primary-fg))]" : "text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
              )}
              aria-label="Tree view"
            >
              <GitBranch size={14} />
            </button>
            <button
              onClick={() => setViewMode("list")}
              className={cn(
                "flex items-center gap-1 px-2 py-1 text-xs",
                viewMode === "list" ? "bg-[rgb(var(--primary))] text-[rgb(var(--primary-fg))]" : "text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
              )}
              aria-label="List view"
            >
              <List size={14} />
            </button>
          </div>
          {isHost && (
            <button
              onClick={() => handleAddTopic(null)}
              className="flex items-center gap-1 rounded bg-[rgb(var(--primary))] px-3 py-1 text-sm font-medium text-[rgb(var(--primary-fg))] hover:opacity-90"
            >
              <Plus size={16} />
              Add topic
            </button>
          )}
        </div>
      </div>

      {rootTopics.length === 0 ? (
        <p className="text-sm text-[rgb(var(--muted))]">
          No topics yet. {isHost ? "Add one to get started." : "Waiting for host to add topics."}
        </p>
      ) : viewMode === "tree" ? (
        <ul>
          {rootTopics.map((topic) => (
            <TreeNode
              key={topic.id}
              topic={topic}
              topics={topics}
              activeTopicId={activeTopicId}
              editingId={editingId}
              onStartEdit={setEditingId}
              onEndEdit={() => setEditingId(null)}
              depth={0}
              isHost={isHost}
              onAddChild={handleAddTopic}
            />
          ))}
        </ul>
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
                  onDelete={null}
                />
              ))}
            </ul>
          </SortableContext>
        </DndContext>
      )}
    </section>
  );
}
