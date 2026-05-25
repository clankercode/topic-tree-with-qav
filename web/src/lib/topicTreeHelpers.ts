import type { Topic } from "../ws/types";

export const MAX_TOPIC_DEPTH = 10;

export function topicDepth(topics: Topic[], topicId: string): number {
  let depth = 0;
  let current: string | null = topicId;
  while (current) {
    depth += 1;
    const t = topics.find((x) => x.id === current);
    if (!t) break;
    current = t.parentId;
  }
  return depth;
}

export function subtreeMaxDepth(topics: Topic[], rootId: string): number {
  const childDepths = topics
    .filter((t) => t.parentId === rootId)
    .map((c) => subtreeMaxDepth(topics, c.id));
  return 1 + (childDepths.length ? Math.max(...childDepths) : 0);
}

export function wouldExceedDepth(
  topics: Topic[],
  topicId: string,
  newParentId: string | null,
): boolean {
  const newRootDepth = newParentId ? topicDepth(topics, newParentId) + 1 : 1;
  const subtree = subtreeMaxDepth(topics, topicId);
  return newRootDepth + subtree - 1 > MAX_TOPIC_DEPTH;
}

export function siblingsOf(topics: Topic[], parentId: string | null): Topic[] {
  return topics
    .filter((t) => t.parentId === parentId)
    .sort((a, b) => a.ord - b.ord);
}

export function previousSibling(topics: Topic[], topic: Topic): Topic | null {
  const siblings = siblingsOf(topics, topic.parentId);
  const idx = siblings.findIndex((s) => s.id === topic.id);
  return idx > 0 ? siblings[idx - 1] : null;
}

export function parentOf(topics: Topic[], topic: Topic): Topic | null {
  if (!topic.parentId) return null;
  return topics.find((t) => t.id === topic.parentId) ?? null;
}
