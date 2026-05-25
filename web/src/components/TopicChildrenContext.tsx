// Shared children-by-parent index for the topic tree. Passed via
// context to avoid prop-drilling the full topic list down every
// `TopicNode` level (the tree is unbounded in depth).
//
// Co-locates the React context value, provider, and a small hook so
// every consumer goes through the same surface. `useTopicChildren`
// is the canonical reader; tests and other components should not
// dip into the context object directly.

/* eslint-disable react-refresh/only-export-components */

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
