import { useCallback, useMemo, useState } from "react";

const STORAGE_PREFIX = "topic-tree-collapsed:";

function loadCollapsed(roomId: string | undefined): Set<string> {
  if (!roomId || typeof localStorage === "undefined") return new Set();
  try {
    const raw = localStorage.getItem(`${STORAGE_PREFIX}${roomId}`);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as string[];
    return new Set(parsed);
  } catch {
    return new Set();
  }
}

function saveCollapsed(roomId: string, ids: Set<string>) {
  localStorage.setItem(`${STORAGE_PREFIX}${roomId}`, JSON.stringify([...ids]));
}

export function useTopicCollapse(roomId: string | undefined) {
  const [collapsed, setCollapsed] = useState<Set<string>>(() =>
    loadCollapsed(roomId),
  );

  const isCollapsed = useCallback(
    (topicId: string) => collapsed.has(topicId),
    [collapsed],
  );

  const toggleCollapse = useCallback(
    (topicId: string) => {
      if (!roomId) return;
      setCollapsed((prev) => {
        const next = new Set(prev);
        if (next.has(topicId)) next.delete(topicId);
        else next.add(topicId);
        saveCollapsed(roomId, next);
        return next;
      });
    },
    [roomId],
  );

  const expand = useCallback(
    (topicId: string) => {
      if (!roomId) return;
      setCollapsed((prev) => {
        if (!prev.has(topicId)) return prev;
        const next = new Set(prev);
        next.delete(topicId);
        saveCollapsed(roomId, next);
        return next;
      });
    },
    [roomId],
  );

  return useMemo(
    () => ({ isCollapsed, toggleCollapse, expand }),
    [isCollapsed, toggleCollapse, expand],
  );
}
