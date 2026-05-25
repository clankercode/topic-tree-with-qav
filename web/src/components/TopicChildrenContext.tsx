// Shared children-by-parent index for the topic tree. Passed via
// context to avoid prop-drilling the full topic list down every
// `TopicNode` level (the tree is unbounded in depth).

import { createContext, useContext, type ReactNode } from "react";
import type { Topic } from "../ws/types";

export interface ChildrenIndex {
  /// `Map<parentId | ROOT_KEY, Topic[]>` already sorted by `ord`.
  map: Map<string, Topic[]>;
  /// Sentinel key used for root-level entries (those with `parentId == null`
  /// or whose `parentId` references a topic missing from the snapshot).
  rootKey: string;
}

const TopicChildrenCtx = createContext<ChildrenIndex | null>(null);

export function TopicChildrenProvider({
  value,
  children,
}: {
  value: ChildrenIndex;
  children: ReactNode;
}) {
  return (
    <TopicChildrenCtx.Provider value={value}>
      {children}
    </TopicChildrenCtx.Provider>
  );
}

export function useTopicChildren(parentId: string): Topic[] {
  const ctx = useContext(TopicChildrenCtx);
  if (!ctx) return [];
  return ctx.map.get(parentId) ?? [];
}
