// Task #13: round-trip + validation tests for the topic tree
// import/export helpers. The "round-trip" case is the load-bearing
// one — it guarantees the format stays a closed loop so an exported
// tree can be re-imported verbatim.

import { describe, expect, it } from "vitest";

import {
  buildExportPayload,
  parseImportPayload,
  TOPIC_TREE_SCHEMA_PROMPT,
} from "../../src/lib/topicTreeExport";
import type { Topic } from "../../src/ws/types";

function topic(
  id: string,
  parentId: string | null,
  ord: number,
  title: string,
): Topic {
  return {
    id,
    parentId,
    title,
    ord,
    status: "pending",
    createdAt: 0,
    voteCount: 0,
  };
}

describe("buildExportPayload", () => {
  it("produces a nested envelope from a flat topic list", () => {
    const envelope = buildExportPayload([
      topic("A", null, 1, "Root A"),
      topic("B", "A", 1, "Child of A"),
      topic("C", "B", 1, "Grandchild of A"),
      topic("D", null, 2, "Root D"),
    ]);
    expect(envelope.version).toBe(1);
    expect(envelope.topics).toHaveLength(2);
    expect(envelope.topics[0].title).toBe("Root A");
    expect(envelope.topics[0].children).toHaveLength(1);
    expect(envelope.topics[0].children[0].title).toBe("Child of A");
    expect(envelope.topics[0].children[0].children[0].title).toBe(
      "Grandchild of A",
    );
    expect(envelope.topics[1].title).toBe("Root D");
  });

  it("sorts siblings by ord (not by insertion order)", () => {
    const envelope = buildExportPayload([
      topic("B", null, 2, "Second"),
      topic("A", null, 1, "First"),
    ]);
    expect(envelope.topics.map((t) => t.title)).toEqual(["First", "Second"]);
  });

  it("falls back orphans-with-missing-parent to the root", () => {
    const envelope = buildExportPayload([
      topic("A", null, 1, "Root"),
      topic("ORPHAN", "GONE", 1, "Stranded"),
    ]);
    const titles = envelope.topics.map((t) => t.title);
    expect(titles).toContain("Stranded");
  });
});

describe("parseImportPayload", () => {
  it("round-trips the buildExportPayload output", () => {
    const exported = buildExportPayload([
      topic("A", null, 1, "Root"),
      topic("B", "A", 1, "Child"),
    ]);
    const result = parseImportPayload(JSON.stringify(exported));
    if (!result.ok) throw new Error("parse failed");
    expect(result.topics).toHaveLength(1);
    expect(result.topics[0].title).toBe("Root");
    expect(result.topics[0].children[0].title).toBe("Child");
  });

  it("defaults status to pending when omitted", () => {
    const json = JSON.stringify({
      version: 1,
      topics: [{ title: "T" }],
    });
    const result = parseImportPayload(json);
    if (!result.ok) throw new Error("parse failed");
    expect(result.topics[0].status).toBe("pending");
  });

  it("rejects invalid JSON", () => {
    const result = parseImportPayload("{not json");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("invalid_json");
  });

  it("rejects an unsupported version", () => {
    const result = parseImportPayload(
      JSON.stringify({ version: 99, topics: [{ title: "T" }] }),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("unsupported_version");
  });

  it("rejects missing topics array", () => {
    const result = parseImportPayload(JSON.stringify({ version: 1 }));
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("missing_topics");
  });

  it("rejects a node with an empty title", () => {
    const result = parseImportPayload(
      JSON.stringify({
        version: 1,
        topics: [{ title: "  " }],
      }),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("invalid_node");
  });

  it("rejects an invalid status value", () => {
    const result = parseImportPayload(
      JSON.stringify({
        version: 1,
        topics: [{ title: "T", status: "in_progress" }],
      }),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.kind).toBe("invalid_node");
      if (result.error.kind === "invalid_node") {
        expect(result.error.message).toContain("status");
      }
    }
  });

  it("recurses into children deeply", () => {
    const json = JSON.stringify({
      version: 1,
      topics: [
        {
          title: "L1",
          children: [
            {
              title: "L2",
              children: [{ title: "L3", children: [{ title: "L4" }] }],
            },
          ],
        },
      ],
    });
    const result = parseImportPayload(json);
    if (!result.ok) throw new Error("parse failed");
    expect(result.topics[0].children[0].children[0].children[0].title).toBe(
      "L4",
    );
  });
});

describe("TOPIC_TREE_SCHEMA_PROMPT", () => {
  it("documents the version and constraints", () => {
    expect(TOPIC_TREE_SCHEMA_PROMPT).toContain("version 1");
    expect(TOPIC_TREE_SCHEMA_PROMPT).toContain("max 500");
    expect(TOPIC_TREE_SCHEMA_PROMPT).toContain("max 10");
  });
});
