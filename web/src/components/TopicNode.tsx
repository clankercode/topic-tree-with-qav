import { useDroppable } from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  Check,
  ChevronDown,
  ChevronRight,
  GripVertical,
  Plus,
} from "lucide-react";
import { useState } from "react";
import { useTopicCollapse } from "../hooks/useTopicCollapse";
import {
  MAX_TOPIC_DEPTH,
  parentOf,
  previousSibling,
  siblingsOf,
  topicDepth,
  wouldExceedDepth,
} from "../lib/topicTreeHelpers";
import { useSessionStore } from "../store";
import { useToastStore } from "../store/toast";
import { sendWsMsg } from "../ws/manager";
import type { Topic } from "../ws/types";
import type { ChildrenIndex } from "./TopicChildrenContext";
import { useTopicChildren } from "./TopicChildrenContext";
import { VoteButton } from "./VoteButton";

interface TopicNodeProps {
  topic: Topic;
  allTopics: Topic[];
  childrenIndex: ChildrenIndex;
  isEditing: boolean;
  onStartEdit: () => void;
  onEndEdit: () => void;
}

function ChildDropTarget({ topicId }: { topicId: string }) {
  const { setNodeRef, isOver } = useDroppable({ id: `child:${topicId}` });
  return (
    <div
      ref={setNodeRef}
      className={`absolute inset-0 rounded ${isOver ? "ring-2 ring-[rgb(var(--primary))] ring-inset" : ""}`}
      aria-hidden
    />
  );
}

export function TopicNode({
  topic,
  allTopics,
  childrenIndex,
  isEditing,
  onStartEdit,
  onEndEdit,
}: TopicNodeProps) {
  const me = useSessionStore((s) => s.me);
  const room = useSessionStore((s) => s.room);
  const activeTopicId = useSessionStore((s) => s.activeTopicId);
  const myTopicVotes = useSessionStore((s) => s.myTopicVotes);
  const optimisticRenameTopic = useSessionStore((s) => s.optimisticRenameTopic);
  const optimisticMoveTopic = useSessionStore((s) => s.optimisticMoveTopic);
  const optimisticAddTopic = useSessionStore((s) => s.optimisticAddTopic);
  const addToast = useToastStore((s) => s.addToast);
  const collapse = useTopicCollapse(room?.id);

  const children = useTopicChildren(topic.id);
  const isHost = me?.role === "host";
  const isGuest = me?.role === "guest";
  const isDone = topic.status === "done";
  const isActive = activeTopicId === topic.id;
  const hasChildren = children.length > 0;
  const collapsed = hasChildren && collapse.isCollapsed(topic.id);

  const [addingSubtopic, setAddingSubtopic] = useState(false);
  const [subtopicTitle, setSubtopicTitle] = useState("");
  const [editingChildId, setEditingChildId] = useState<string | null>(null);

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

  function handleIndent() {
    const prev = previousSibling(allTopics, topic);
    if (!prev) return;
    if (wouldExceedDepth(allTopics, topic.id, prev.id)) {
      addToast("Topic tree cannot exceed 10 levels deep.", "error");
      return;
    }
    const lastChild = siblingsOf(allTopics, prev.id).at(-1);
    sendMove(topic.id, prev.id, lastChild?.id ?? null);
    collapse.expand(prev.id);
  }

  function handleOutdent() {
    const parent = parentOf(allTopics, topic);
    const newParentId = parent?.parentId ?? null;
    if (wouldExceedDepth(allTopics, topic.id, newParentId)) {
      addToast("Topic tree cannot exceed 10 levels deep.", "error");
      return;
    }
    sendMove(topic.id, newParentId, parent?.id ?? null);
  }

  function handleAddSubtopic() {
    const title = subtopicTitle.trim();
    if (!title || !isHost) return;
    const depth = topicDepth(allTopics, topic.id);
    if (depth >= MAX_TOPIC_DEPTH) {
      addToast("Topic tree cannot exceed 10 levels deep.", "error");
      return;
    }
    const siblings = siblingsOf(allTopics, topic.id);
    const tempId = crypto.randomUUID();
    const afterId =
      siblings.length > 0 ? siblings[siblings.length - 1].id : null;
    optimisticAddTopic(tempId, topic.id, title, afterId);
    sendWsMsg({
      v: 1,
      type: "AddTopic",
      id: tempId,
      parentId: topic.id,
      title,
      afterId,
    });
    collapse.expand(topic.id);
    setSubtopicTitle("");
    setAddingSubtopic(false);
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
      <div className="relative flex items-center gap-2 px-3 py-2">
        {isHost && <ChildDropTarget topicId={topic.id} />}

        {hasChildren && (
          <button
            type="button"
            onClick={() => collapse.toggleCollapse(topic.id)}
            className="relative z-10 text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
            aria-label={collapsed ? "Expand subtopics" : "Collapse subtopics"}
          >
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
          </button>
        )}

        {isHost && (
          <button
            type="button"
            {...attributes}
            {...listeners}
            className="relative z-10 cursor-grab touch-none text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
            aria-label="Drag to reorder"
          >
            <GripVertical size={16} />
          </button>
        )}

        {isGuest && (
          <div className="relative z-10">
            <VoteButton
              target={{ kind: "topic", id: topic.id }}
              voteCount={topic.voteCount ?? 0}
              hasVoted={myTopicVotes.has(topic.id)}
              faded={isDone}
            />
          </div>
        )}

        {isDone && (
          <button
            type="button"
            onClick={handleDoneToggle}
            className="relative z-10 flex h-5 w-5 items-center justify-center rounded bg-[rgb(var(--success))] text-[rgb(var(--success-fg))]"
            aria-label="Mark as pending"
          >
            <Check size={12} />
          </button>
        )}

        {isActive && !isDone && (
          <span className="relative z-10 flex h-5 items-center rounded bg-[rgb(var(--primary))] px-2 text-xs font-medium text-[rgb(var(--primary-fg))]">
            Active
          </span>
        )}

        {isEditing ? (
          <input
            type="text"
            defaultValue={topic.title}
            autoFocus
            className="relative z-10 flex-1 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-2 py-1 text-sm"
            onBlur={(e) => handleRename(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                handleRename((e.target as HTMLInputElement).value);
              } else if (e.key === "Escape") {
                onEndEdit();
              } else if (e.key === "Tab") {
                e.preventDefault();
                if (e.shiftKey) handleOutdent();
                else handleIndent();
              }
            }}
          />
        ) : (
          <span
            className={`relative z-10 flex-1 cursor-pointer text-sm ${isDone ? "text-[rgb(var(--muted))] line-through" : ""}`}
            onClick={!isHost && !isDone ? handleSetActive : undefined}
            onDoubleClick={isHost ? onStartEdit : undefined}
          >
            {topic.title}
          </span>
        )}

        {collapsed && hasChildren && (
          <span className="relative z-10 text-xs text-[rgb(var(--muted))]">
            {children.length} subtopic{children.length === 1 ? "" : "s"}
          </span>
        )}

        {!isDone && !isActive && !isEditing && isHost && (
          <button
            type="button"
            onClick={handleDoneToggle}
            className="relative z-10 text-xs text-[rgb(var(--muted))] hover:text-[rgb(var(--success))]"
            aria-label="Mark as done"
          >
            Done
          </button>
        )}

        {isHost && !isActive && !isEditing && (
          <>
            <button
              type="button"
              onClick={() => setAddingSubtopic(true)}
              className="relative z-10 text-xs text-[rgb(var(--muted))] hover:text-[rgb(var(--primary))]"
              aria-label="Add subtopic"
            >
              <Plus size={14} />
            </button>
            <button
              type="button"
              onClick={onStartEdit}
              className="relative z-10 text-xs text-[rgb(var(--muted))] hover:text-[rgb(var(--foreground))]"
              aria-label="Rename topic"
            >
              Rename
            </button>
          </>
        )}
      </div>

      {addingSubtopic && (
        <div className="flex gap-2 border-t border-[rgb(var(--border))] px-3 py-2">
          <input
            type="text"
            value={subtopicTitle}
            onChange={(e) => setSubtopicTitle(e.target.value)}
            placeholder="Subtopic title"
            autoFocus
            className="flex-1 rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-2 py-1 text-sm"
            onKeyDown={(e) => {
              if (e.key === "Enter") handleAddSubtopic();
              if (e.key === "Escape") {
                setAddingSubtopic(false);
                setSubtopicTitle("");
              }
            }}
          />
          <button
            type="button"
            onClick={handleAddSubtopic}
            className="rounded bg-[rgb(var(--primary))] px-2 py-1 text-xs text-[rgb(var(--primary-fg))]"
          >
            Add
          </button>
        </div>
      )}

      {hasChildren && !collapsed && (
        <SortableContext
          items={children.map((c) => c.id)}
          strategy={verticalListSortingStrategy}
        >
          <ul
            className="ml-6 mb-2 space-y-2 border-l border-[rgb(var(--border))] pl-3"
            aria-label={`Subtopics of ${topic.title}`}
          >
            {children.map((child) => (
              <TopicNode
                key={child.id}
                topic={child}
                allTopics={allTopics}
                childrenIndex={childrenIndex}
                isEditing={editingChildId === child.id}
                onStartEdit={() => setEditingChildId(child.id)}
                onEndEdit={() => setEditingChildId(null)}
              />
            ))}
          </ul>
        </SortableContext>
      )}
    </li>
  );
}
