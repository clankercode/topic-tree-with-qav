# Whiteboards

Two flavors, host chooses per board. Both render inside the same `BoardCanvas` host component which dispatches based on `board.kind`.

## 1. Pen board

### Components

```
PenBoard
├── PenCanvas         (HTMLCanvasElement, drawing layer)
├── PenTextLayer      (absolute-positioned <div>s for text)
├── PenToolPalette    (host only: pen color, size, text-tool, undo, clear)
├── CursorLayer       (remote cursors)
└── ClickPingLayer    (transient click pings)
```

### Drawing pipeline

```
pointerdown → strokeId = uuid                            (host)
  send PenStrokeBegin {boardId, strokeId, color, size}
  local: push new Stroke into local store with points=[]
pointermove (rAF-throttled, batched)
  batched points → send PenStrokeAppend {boardId, strokeId, points}
  local: append points; redraw the in-progress segment via perfect-freehand getStroke()
pointerup
  send PenStrokeEnd {boardId, strokeId}
  local: finalize stroke
```

- **`perfect-freehand`** converts raw `{x,y,pressure}` points into smooth outline polygons. We render as filled paths on canvas.
- Stroke storage on server: array of `{x,y,pressure}` triples. ~30 floats per stroke for typical pen motion = tiny.
- **Replay on client**: when a guest joins or refocuses, we receive the full stroke list and render in order.

### Smoothness over the wire

- Sender: batch points captured during one animation frame into one ws message. Cursor effective update rate ≈ display refresh / batch size.
- Receiver: render incoming `PenStrokeAppended` events directly via the same `getStroke()`. There is no animation interpolation needed — the *points themselves* are smooth from perfect-freehand at the sender. Latency is the only visible artifact (typically 50-200ms on a regional Railway deploy).
- If receiver-side smoothing becomes needed later, we can interpolate between successive point batches over their arrival interval. Defer.

### Text tool

- Click empty space → spawn a textbox input. On commit (Enter or blur), send `PenTextSet`.
- Click an existing text → edit. Same event.
- Delete: select + Backspace → `PenTextDelete`.
- Text is stored separately from strokes; renders in HTML for crispness and accessibility (selectable, copy-pasteable).

### Undo / Clear

- `PenUndo` removes the last *stroke or text* (per board, monotonic `ord`).
- `PenClear` wipes both strokes and texts for the board (with confirm dialog).
- Undo history is server-side authoritative; the last 50 actions per board are kept in-memory for fast undo; SQLite holds the full record but undo doesn't undelete past 50 (call it out in tooltip).

## 2. Excalidraw board

### Embedding

- Use `@excalidraw/excalidraw` (MIT). Mount via `<Excalidraw ref onChange={...} viewModeEnabled={!isHost} />`.
- `viewModeEnabled=true` for guests → makes Excalidraw read-only natively; toolbar hidden, pointer hover/click still work.
- For host: full editing.

### Sync model — server as relay

Excalidraw's recommended collab pattern: clients exchange the scene's *elements array* plus optional `appState`. We adopt this verbatim but route through our ws server, not their P2P channel.

```
host onChange (debounced 150ms):
  → send ExcalidrawUpdate {boardId, sceneVersion, elements, appState}
server:
  → persist elements_json (atomic write to excalidraw_scenes table)
  → broadcast ExcalidrawDelta to all other clients in room
guest receives ExcalidrawDelta:
  → call excalidrawAPI.updateScene({elements, appState})
```

Only the host sends `ExcalidrawUpdate` because `viewModeEnabled=true` on guests prevents edits. We *still* assert admin auth server-side for defense in depth.

### Pointer broadcasts

Excalidraw exposes a collaborator pointer API:

- Host (and any guest opting in) sends `Cursor {boardId, x, y}` (already in our universal protocol).
- Server broadcasts to others as `CursorMoved`.
- The board component calls `excalidrawAPI.updateScene({collaborators: Map<userId, {pointer:{x,y}, username}>})` each frame with the latest cursors.

### Late joiners

- `Welcome` snapshot includes the latest `{sceneVersion, elements, appState}` for each Excalidraw board.
- On focus, we mount Excalidraw with `initialData={elements, appState}`.

### Why not the official collab server?

- Their public collab server is for their hosted app; self-hosting it requires a Node service we don't otherwise need.
- The collab protocol surface we need is small (broadcast a JSON blob of scene state) — replicating it in our Rust relay is ~50 LOC.
- Avoiding a Node sidecar keeps the deployment one binary + one volume.

## 3. Shared concerns

### Focused board

- `focusedBoardId` is a single server-owned field per room.
- Host can change it via `SetFocusedBoard`. Server broadcasts `FocusedBoardChanged`.
- Each guest has a local "Follow host" toggle (default ON for new joiners). If ON, their local `viewBoardId` mirrors `focusedBoardId`. If OFF, they pick from the BoardTabs strip.

### Cursors

- Both board types share `CursorLayer`, a single absolutely-positioned SVG/HTML layer.
- Cursors interpolate position over 50ms for visual smoothness despite 20Hz updates.
- A cursor disappears if no update for 5s.

### Click pings

- Any client can send `Click {boardId, x, y}`. Server rate-limits to 5/s/client.
- Server broadcasts `Clicked` to *all* clients including sender (so sender sees same visual feedback as others).
- Renders as an expanding ring + name label fading over 1.2s.

### Coordinate space

- Pen board: canvas-internal coords (we set canvas size to a fixed virtual 4096×2304; CSS-scale to viewport, letterboxed; this keeps strokes resolution-independent).
- Excalidraw: its own scene coords. Cursor pings on Excalidraw are sent in scene coords so they remain anchored to scene content during pan/zoom.
- The protocol uses generic `{x, y}` floats; the board component knows how to interpret them per kind.

### Performance budget

- Up to 50 simultaneous viewers per room.
- Up to 30 cursor updates/s per active client, ~6 active drawers (host + delegates if ever added) — leaving ~1500 cursor msgs/s peak, well within ws fanout.
- Stroke points: 60 batches/s × ~10 points × 6 bytes/point JSON-encoded ≈ 4KB/s per drawer = trivial.
