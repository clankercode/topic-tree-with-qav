import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Check, GripVertical, Plus, Trash2, ChevronRight, ChevronDown } from "lucide-react";
import { useState } from "react";
import { useSessionStore } from "../store";
import { sendWsMsg } from "../ws/manager";
import type { Topic } from "../ws/types";
import { cn } from "../lib/utils";

interface TopicNodeProps {
  topic: Topic;
  isActive: boolean;
  isEditing: boolean;
  onStartEdit: () => void;
  onEndEdit: () => void;
  onDelete?: (() => void) | null;
  showAddChild?: boolean;
  onAddChild?: (() => void) | null;
  hasChildren?: boolean;
  isExpanded?: boolean;
  onToggleExpand?: () => void;
}

export function TopicNode({
  topic,
  isActive,
  isEditing,
  onStartEdit,
  onEndEdit,
  onDelete,
  showAddChild,
  onAddChild,
  hasChildren,
  isExpanded,
  onToggleExpand,
}: TopicNodeProps) {
  const { me, optimisticRenameTopic } = useSessionStore();
  const isHost = me?.role === "host";
  const isDone = topic.status === "done";
  const [internalExpanded, setInternalExpanded] = useState(true);
  const expanded = isExpanded !== undefined ? isExpanded : internalExpanded;
  const toggleExpand = onToggleExpand || (() => setInternalExpanded(!internalExpanded));

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
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        "flex items-center gap-2",
        isDragging && "opacity-50 shadow-lg",
        isActive && "font-medium",
        isDone && "opacity-60",
      )}
    >
      {hasChildren && (
        <button
          onClick={toggleExpand}
          className="flex h-5 w-5 items-center justify-center text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
          aria-label={expanded ? "Collapse" : "Expand"}
        >
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
      )}
      {!hasChildren && <span className="h-5 w-5" />}

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
            if (e.key === "Enter") handleRename((e.target as HTMLInputElement).value);
            if (e.key === "Escape") onEndEdit();
          }}
        />
      ) : (
        <span
          className={cn("flex-1 cursor-pointer text-sm", isDone && "text-[rgb(var(--muted))] line-through")}
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

      {isHost && showAddChild && onAddChild && (
        <button
          onClick={onAddChild}
          className="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-[rgb(var(--muted))] hover:bg-[rgb(var(--border))] hover:text-[rgb(var(--foreground))]"
          aria-label="Add subtopic"
        >
          <Plus size={12} />
          Sub
        </button>
      )}

      {isHost && onDelete && (
        <button
          onClick={onDelete}
          className="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-[rgb(var(--muted))] hover:bg-red-500/20 hover:text-red-500"
          aria-label="Delete topic"
        >
          <Trash2 size={12} />
        </button>
      )}
    </div>
  );
}
