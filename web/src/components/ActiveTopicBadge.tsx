import { useSessionStore } from "../store";

export function ActiveTopicBadge() {
  const { topics, activeTopicId } = useSessionStore();

  const activeTopic = topics.find((t) => t.id === activeTopicId);
  if (!activeTopic) return null;

  return (
    <div className="flex items-center gap-2 rounded bg-[rgb(var(--primary))] px-3 py-1 text-sm font-medium text-[rgb(var(--primary-fg))]">
      <span className="relative flex h-2 w-2">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-[rgb(var(--primary-fg))] opacity-75"></span>
        <span className="relative inline-flex h-2 w-2 rounded-full bg-[rgb(var(--primary-fg))]"></span>
      </span>
      {activeTopic.title}
    </div>
  );
}
