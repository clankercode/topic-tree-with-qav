// Task #13: client-side helpers for the topic-tree export / import
// pair. The export shape is the same `ImportedTopicNode` the server
// understands, so a round-trip is just JSON in -> JSON out.
//
// Schema is intentionally minimal:
//
//   {
//     "version": 1,
//     "topics": [
//       {
//         "title": "Root A",
//         "status": "pending" | "done",
//         "children": [ ...same shape... ]
//       }
//     ]
//   }
//
// `version` is forward-looking; the importer accepts only `1` today
// and will error loudly on a mismatch so future schema changes
// surface immediately.

import type { ImportedTopicNode, TopicStatus } from "../proto/generated";
import type { Topic } from "../ws/types";

export interface ExportEnvelope {
  version: 1;
  topics: ImportedTopicNode[];
}

export const TOPIC_TREE_EXPORT_VERSION = 1;

/// Human-readable schema description that the "Copy schema" button
/// hands to LLM agents so they can author a valid import payload.
export const TOPIC_TREE_SCHEMA_PROMPT = `Topic tree import schema for topic-tree-with-qav (version 1).

The payload is a JSON object:

{
  "version": 1,
  "topics": [
    {
      "title": "string, 1..=200 chars, non-empty after trim",
      "status": "pending" | "done",   // optional, defaults to "pending"
      "children": [ /* same shape, recursive */ ]
    }
  ]
}

Constraints (enforced server-side):
- topics array must be non-empty (>= 1 root)
- total node count across the whole tree: max 500
- max depth (root counts as 1): max 10
- each title must be 1..=200 chars after trimming

The server generates fresh UUIDs for every imported node, so do NOT
include id fields. Parent-child relationships are inferred from
nesting: an entry inside another's "children" array becomes its
child.

Status values are lower-case literals.

Example payload:

{
  "version": 1,
  "topics": [
    {
      "title": "Plenary",
      "status": "pending",
      "children": [
        { "title": "Intro", "status": "done", "children": [] },
        { "title": "Deep dive", "status": "pending", "children": [] }
      ]
    }
  ]
}
`;

/// Build an export envelope from the flat topic list held in the
/// session store. Sorts every sibling group by `ord` so the export
/// reflects on-screen ordering.
export function buildExportPayload(topics: Topic[]): ExportEnvelope {
  const childrenByParent = new Map<string, Topic[]>();
  const known = new Set(topics.map((t) => t.id));
  const ROOT = "__root__";
  for (const t of topics) {
    const key =
      t.parentId == null || !known.has(t.parentId) ? ROOT : t.parentId;
    const existing = childrenByParent.get(key);
    if (existing) existing.push(t);
    else childrenByParent.set(key, [t]);
  }
  for (const list of childrenByParent.values())
    list.sort((a, b) => a.ord - b.ord);

  function buildNode(t: Topic): ImportedTopicNode {
    return {
      title: t.title,
      status: t.status,
      children: (childrenByParent.get(t.id) ?? []).map(buildNode),
    };
  }
  return {
    version: TOPIC_TREE_EXPORT_VERSION,
    topics: (childrenByParent.get(ROOT) ?? []).map(buildNode),
  };
}

export type ParseError =
  | { kind: "invalid_json"; message: string }
  | { kind: "missing_version" }
  | { kind: "unsupported_version"; got: unknown }
  | { kind: "missing_topics" }
  | { kind: "invalid_node"; path: string; message: string };

export type ParseResult =
  | { ok: true; topics: ImportedTopicNode[] }
  | { ok: false; error: ParseError };

/// Parse + validate a JSON string into a list of ImportedTopicNode.
/// Returns either a successful Vec or a structured ParseError so the
/// UI can surface a precise message.
export function parseImportPayload(
  raw: string,
):
  | { ok: true; topics: ImportedTopicNode[] }
  | { ok: false; error: ParseError } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    return {
      ok: false,
      error: { kind: "invalid_json", message: (e as Error).message },
    };
  }
  if (typeof parsed !== "object" || parsed === null) {
    return { ok: false, error: { kind: "missing_version" } };
  }
  const env = parsed as Record<string, unknown>;
  if (!("version" in env))
    return { ok: false, error: { kind: "missing_version" } };
  if (env.version !== TOPIC_TREE_EXPORT_VERSION) {
    return {
      ok: false,
      error: { kind: "unsupported_version", got: env.version },
    };
  }
  if (!Array.isArray(env.topics)) {
    return { ok: false, error: { kind: "missing_topics" } };
  }
  const validated: ImportedTopicNode[] = [];
  for (let i = 0; i < env.topics.length; i++) {
    const result = validateNode(env.topics[i], `topics[${i}]`);
    if (!result.ok) return result;
    validated.push(result.node);
  }
  return { ok: true, topics: validated };
}

function validateNode(
  raw: unknown,
  path: string,
): { ok: true; node: ImportedTopicNode } | { ok: false; error: ParseError } {
  if (typeof raw !== "object" || raw === null) {
    return {
      ok: false,
      error: { kind: "invalid_node", path, message: "must be an object" },
    };
  }
  const r = raw as Record<string, unknown>;
  if (typeof r.title !== "string" || r.title.trim() === "") {
    return {
      ok: false,
      error: {
        kind: "invalid_node",
        path,
        message: "missing or empty title",
      },
    };
  }
  let status: TopicStatus = "pending";
  if (r.status !== undefined) {
    if (r.status !== "pending" && r.status !== "done") {
      return {
        ok: false,
        error: {
          kind: "invalid_node",
          path,
          message: `status must be "pending" or "done", got ${JSON.stringify(r.status)}`,
        },
      };
    }
    status = r.status;
  }
  const children: ImportedTopicNode[] = [];
  if (r.children !== undefined) {
    if (!Array.isArray(r.children)) {
      return {
        ok: false,
        error: {
          kind: "invalid_node",
          path,
          message: "children must be an array",
        },
      };
    }
    for (let i = 0; i < r.children.length; i++) {
      const childResult = validateNode(r.children[i], `${path}.children[${i}]`);
      if (!childResult.ok) return childResult;
      children.push(childResult.node);
    }
  }
  return {
    ok: true,
    node: { title: r.title, status, children },
  };
}
