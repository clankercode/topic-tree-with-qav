// Task #13: Export / Import / Copy-schema affordance for the host.
// Lives next to the "Add topic" header button inside TopicTree.
//
// Three flows:
//   - **Export**: downloads the current tree as a JSON file. The file
//     name is `topic-tree-<room-id>-<timestamp>.json`.
//   - **Copy schema**: copies a prose description of the import
//     schema to the clipboard so the user can paste it into an LLM
//     and get back a generated payload.
//   - **Import**: opens a textarea modal; user pastes a JSON payload;
//     on Submit the payload is validated client-side, then sent as a
//     single `ImportTopicTree` ws frame. Server validates again +
//     atomically creates every node.

import { useEffect, useId, useRef, useState } from "react";
import { Download, Upload, ClipboardCopy } from "lucide-react";
import { useSessionStore } from "../store";
import { useToastStore } from "../store/toast";
import { registerPendingSubmit, sendWsMsg } from "../ws/manager";
import {
  buildExportPayload,
  parseImportPayload,
  TOPIC_TREE_SCHEMA_PROMPT,
  type ParseError,
} from "../lib/topicTreeExport";
import { useModalFocus } from "./useModalFocus";

async function copyToClipboard(text: string): Promise<boolean> {
  if (
    typeof navigator !== "undefined" &&
    navigator.clipboard?.writeText !== undefined
  ) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // fall through
    }
  }
  return false;
}

export function TopicTreeImportExport() {
  const topics = useSessionStore((s) => s.topics);
  const room = useSessionStore((s) => s.room);
  const me = useSessionStore((s) => s.me);
  const addToast = useToastStore((s) => s.addToast);
  const [importOpen, setImportOpen] = useState(false);
  const [pasted, setPasted] = useState("");
  const [importError, setImportError] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const pendingImportCleanupRef = useRef<(() => void) | null>(null);
  const titleId = useId();
  useModalFocus(importOpen, dialogRef, () => setImportOpen(false), textareaRef);

  useEffect(
    () => () => {
      pendingImportCleanupRef.current?.();
    },
    [],
  );

  if (me?.role !== "host") return null;

  async function handleExport() {
    const envelope = buildExportPayload(topics);
    const json = JSON.stringify(envelope, null, 2);
    const roomSlug = room?.id ?? "topic-tree";
    const fname = `topic-tree-${roomSlug}-${Date.now()}.json`;
    try {
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = fname;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      addToast(`Exported ${envelope.topics.length} root topic(s).`, "info");
    } catch (e) {
      addToast(`Export failed: ${(e as Error).message}`, "error");
    }
  }

  async function handleCopySchema() {
    const ok = await copyToClipboard(TOPIC_TREE_SCHEMA_PROMPT);
    if (ok) {
      addToast("Import-schema prompt copied to clipboard.", "info");
    } else {
      addToast("Could not access clipboard — see console.", "error");
      console.info(TOPIC_TREE_SCHEMA_PROMPT);
    }
  }

  function handleImportSubmit() {
    const result = parseImportPayload(pasted);
    if (!result.ok) {
      const msg = describeError(result.error);
      setImportError(msg);
      addToast(`Import failed: ${msg}`, "error");
      return;
    }
    const refId = crypto.randomUUID();
    const rootCount = result.topics.length;
    pendingImportCleanupRef.current?.();
    setIsImporting(true);
    setImportError(null);
    pendingImportCleanupRef.current = registerPendingSubmit(
      refId,
      (outcome) => {
        setIsImporting(false);
        pendingImportCleanupRef.current = null;
        if (outcome.kind === "ack") {
          setImportOpen(false);
          setPasted("");
          setImportError(null);
          addToast(`Imported ${rootCount} root topic(s).`, "success");
          return;
        }
        setImportError(outcome.message);
        addToast(outcome.message, "error");
      },
    );
    sendWsMsg({
      v: 1,
      type: "ImportTopicTree",
      id: refId,
      topics: result.topics,
      parentTopicId: null,
    });
  }

  return (
    <>
      <div
        className="flex items-center gap-1"
        aria-label="Topic-tree import-export"
      >
        <button
          type="button"
          onClick={handleExport}
          className="flex items-center gap-1 rounded border border-[rgb(var(--border))] px-2 py-1 text-xs text-[rgb(var(--muted))] hover:border-[rgb(var(--accent))] hover:text-[rgb(var(--accent))]"
          title="Download the current topic tree as JSON"
          aria-label="Export topic tree"
        >
          <Download size={12} />
          Export
        </button>
        <button
          type="button"
          onClick={() => {
            setImportError(null);
            setImportOpen(true);
          }}
          className="flex items-center gap-1 rounded border border-[rgb(var(--border))] px-2 py-1 text-xs text-[rgb(var(--muted))] hover:border-[rgb(var(--accent))] hover:text-[rgb(var(--accent))]"
          title="Import a topic tree from JSON"
          aria-label="Import topic tree"
        >
          <Upload size={12} />
          Import
        </button>
        <button
          type="button"
          onClick={handleCopySchema}
          className="flex items-center gap-1 rounded border border-[rgb(var(--border))] px-2 py-1 text-xs text-[rgb(var(--muted))] hover:border-[rgb(var(--accent))] hover:text-[rgb(var(--accent))]"
          title="Copy the import-schema prompt to clipboard (for sharing with an LLM)"
          aria-label="Copy import schema"
        >
          <ClipboardCopy size={12} />
          Schema
        </button>
      </div>

      {importOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          onClick={() => setImportOpen(false)}
        >
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            tabIndex={-1}
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-xl space-y-3 rounded-lg border border-[rgb(var(--border))] bg-[rgb(var(--surface))] p-5"
          >
            <h2 id={titleId} className="text-base font-semibold">
              Import topic tree
            </h2>
            <p className="text-xs text-[rgb(var(--muted))]">
              Paste a JSON payload below. Use the &quot;Schema&quot; button to
              copy a prompt you can hand to an LLM for generation.
            </p>
            <textarea
              ref={textareaRef}
              value={pasted}
              onChange={(e) => {
                setPasted(e.target.value);
                setImportError(null);
              }}
              placeholder={'{"version":1,"topics":[…]}'}
              rows={12}
              spellCheck={false}
              className="w-full resize-y rounded border border-[rgb(var(--border))] bg-[rgb(var(--background))] px-2 py-1 font-mono text-xs"
            />
            {importError && (
              <p className="text-xs text-red-600" role="alert">
                {importError}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setImportOpen(false)}
                className="rounded border border-[rgb(var(--border))] px-3 py-1 text-xs hover:bg-[rgb(var(--muted))/10]"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleImportSubmit}
                disabled={pasted.trim().length === 0 || isImporting}
                className="rounded bg-[rgb(var(--primary))] px-3 py-1 text-xs text-[rgb(var(--primary-fg))] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {isImporting ? "Importing..." : "Import"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function describeError(e: ParseError): string {
  switch (e.kind) {
    case "invalid_json":
      return `not valid JSON (${e.message})`;
    case "missing_version":
      return 'missing top-level "version" field';
    case "unsupported_version":
      return `unsupported version: ${JSON.stringify(e.got)} (expected 1)`;
    case "missing_topics":
      return 'missing top-level "topics" array';
    case "invalid_node":
      return `${e.path}: ${e.message}`;
  }
}
