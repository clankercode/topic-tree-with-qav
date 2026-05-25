import { useState } from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Check, GripVertical } from "lucide-react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import type { Topic } from "../ws/types";
import { useTopicChildren } from "./TopicChildrenContext";

interface TopicNodeProps {
  topic: Topic;
  isActive: boolean;
  isEditing: boolean;
  onStartEdit: () => void;
  onEndEdit: () => void;
}

export function TopicNode({
  topic,
  isActive,
  isEditing,
  onStartEdit,
  onEndEdit,
}: TopicNodeProps) {
  const me = useSessionStore((s) => s.me);
  const optimisticRenameTopic = useSessionStore((s) => s.optimisticRenameTopic);
  const activeTopicId = useSessionStore((s) => s.activeTopicId);
  const [editingChildId, setEditingChildId] = useState<string | null>(null);
  const children = useTopicChildren(topic.id);
  const isHost = me?.role === "host";
  const isDone = topic.status === "done";

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: topic.id, disabled: !isHost });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  function handleDoneToggle() {
    if (!isHost) return;
    sendWsMsg({
      v: 1,
      type: "MarkTopicDone",
      topicId: topic.id,
      done: !isDone,
    });
  }

  function handleSetActive() {
    if (!isHost) return;
    sendWsMsg({
      v: 1,
      type: "SetActiveTopic",
      topicId: topic.id,
    });
  }

  function handleRename(newTitle: string) {
    if (!isHost || !newTitle.trim()) return;
    optimisticRenameTopic(topic.id, newTitle.trim());
    sendWsMsg({
      v: 1,
      type: "RenameTopic",
      topicId: topic.id,
      title: newTitle.trim(),
    });
    onEndEdit();
  }

  return (
    <li
      ref={setNodeRef}
      style={style}
      className={`flex flex-col rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] ${
        isDragging ? "opacity-50 shadow-lg" : ""
      } ${isActive ? "border-[rgb(var(--primary))] ring-2 ring-[rgb(var(--primary))]" : ""} ${
        isDone ? "opacity-60" : ""
      }`}
    >
      <div className="flex items-center gap-2 px-3 py-2">
        {isHost && (
          <button
            {...attributes}
            {...listeners}
            className="cursor-grab touch-none text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
            aria-label="Drag to reorder"
          >
            <GripVertical size={16} />
          </button>
        )}

        {isDone && (
          <button
            onClick={handleDoneToggle}
            className="flex h-5 w-5 items-center justify-center rounded bg-[rgb(var(--success))] text-[rgb(var(--success-fg))]"
            aria-label="Mark as pending"
          >
            <Check size={12} />
          </button>
        )}

        {isActive && !isDone && (
          <span className="flex h-5 items-center rounded bg-[rgb(var(--primary))] px-2 text-xs font-medium text-[rgb(var(--primary-fg))]">
            Active
          </span>
        )}

        {isEditing ? (
          <input
            type="text"
            defaultValue={topic.title}
            autoFocus
            className="flex-1 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-2 py-1 text-sm"
            onBlur={(e) => handleRename(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter")
                handleRename((e.target as HTMLInputElement).value);
              if (e.key === "Escape") onEndEdit();
            }}
          />
        ) : (
          <span
            className={`flex-1 cursor-pointer text-sm ${isDone ? "text-[rgb(var(--muted))] line-through" : ""}`}
            onClick={!isHost && !isDone ? handleSetActive : undefined}
          >
            {topic.title}
          </span>
        )}

        {!isDone && !isActive && !isEditing && (
          <button
            onClick={handleDoneToggle}
            className="text-xs text-[rgb(var(--muted))] hover:text-[rgb(var(--success))]"
            aria-label="Mark as done"
          >
            Done
          </button>
        )}

        {isHost && !isActive && !isEditing && (
          <button
            onClick={onStartEdit}
            className="text-xs text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
            aria-label="Rename topic"
          >
            Rename
          </button>
        )}
      </div>

      {children.length > 0 && (
        <ul
          className="ml-6 mb-2 space-y-2 border-l border-[rgb(var(--border))] pl-3"
          aria-label={`Subtopics of ${topic.title}`}
        >
          {children.map((child) => (
            <TopicNode
              key={child.id}
              topic={child}
              isActive={activeTopicId === child.id}
              isEditing={editingChildId === child.id}
              onStartEdit={() => setEditingChildId(child.id)}
              onEndEdit={() => setEditingChildId(null)}
            />
          ))}
        </ul>
      )}
    </li>
  );
}
