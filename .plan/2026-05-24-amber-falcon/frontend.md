# Frontend

## 1. Routes

| Path | Purpose |
|---|---|
| `/` | Landing — "Create room" CTA; shows recently created/joined rooms from IndexedDB |
| `/r/new` | POST create flow (server-side or via `/api/rooms` then redirect) |
| `/r/:roomId` | Guest entry — name prompt then session view |
| `/r/:roomId/host` | Host view (gated by IndexedDB-stored adminToken; redirects to claim flow if missing) |
| `/r/:roomId?admin=<token>` | First-time admin claim — strips query, saves to IndexedDB, redirects to `/r/:roomId/host` |
| `/rooms` | Dashboard of all rooms this device knows about (admin + joined-as-guest) |
| `/about` | (stub) |

## 2. Session view layout

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  Topbar:  RoomTitle · Presence count · Theme toggle · "Your name: …" · MenuBtn   │
├───────────────┬──────────────────────────────────────────┬─────────────────────┤
│               │                                          │                     │
│  Topic Tree   │                                          │   Q&A panel         │
│  (collapsible)│        Focused Board (canvas)            │   - "Ask a Q" input │
│               │                                          │   - sort toggle     │
│               │   [board tabs along top]                 │   - autoscroll lock │
│               │                                          │   - jump top/bottom │
│               │                                          │                     │
└───────────────┴──────────────────────────────────────────┴─────────────────────┘
```

- Three-column layout: left (topic tree), center (whiteboard), right (Q&A).
- Each column is independently collapsible. Defaults differ for host vs guest:
  - **Host**: all three open.
  - **Guest**: tree open, board open, Q&A open *but* with an emphasized "Ask" button.
- On narrow screens (<900px): a tabbed view with bottom segmented control (Tree / Board / Q&A).
- The board panel always shows: a tab strip of all boards + "Follow host" toggle (default on for guests).

## 3. Component inventory

**Layout**: `RootLayout`, `SessionLayout`, `ColumnSplitter`, `MobileTabBar`

**Topic tree**: `TopicTree`, `TopicNode`, `TopicEditPopover`, `ActiveTopicBadge`, `TopicStatusIndicator`

**Q&A**: `QAPanel`, `QuestionList`, `QuestionItem`, `QuestionComposer`, `VoteButton`, `SortToggle`, `AutoscrollLock`, `JumpButtons`

**Whiteboard**: `BoardCanvas`, `PenBoard`, `ExcalidrawBoard`, `BoardTabs`, `FollowHostToggle`, `CursorLayer`, `ClickPing`, `ToolPalette` (host only)

**Host controls**: `HostMenu`, `ModerationPanel`, `RoomSettings`

**Shared**: `Avatar`, `NameInput`, `EmptyState`, `Toast`, `ThemeToggle`, `LoadingDots`, `Banner`

## 4. State management

- **Zustand** for client state (single store with slices: `room`, `topics`, `questions`, `boards`, `ui`, `me`).
- WS messages drive store updates via a thin dispatcher in `ws/index.ts`.
- React Query is **not** used — everything important comes through ws after the initial Welcome snapshot.

## 5. Design language

> Brief: "Modern, calm, focused — content forward, chrome minimal. A teaching tool, not a chat app."

- **Typography**: Inter (variable) for UI; JetBrains Mono for IDs/code; large generous line-height in Q&A.
- **Color**: neutral grayscale base with a single accent. Default accent: indigo-500 (light) / indigo-400 (dark). Pick once, use everywhere.
- **Surfaces**: layered with subtle shadows in light; subtle inner-stroke in dark. No glassmorphism.
- **Density**: comfortable on desktop, tight on mobile.
- **Motion**: 150ms ease-out for entrances, 100ms for state changes. Strokes draw at native canvas rate (no rAF capping that). Cursor positions interpolate over 50ms to look smooth despite 20Hz updates.
- **Iconography**: lucide-react.
- **shadcn/ui** primitives: Button, Input, Dialog, Popover, Tabs, ScrollArea, Tooltip, Toast, Card, Switch, DropdownMenu.

> See `frontend-design` skill — recommend invoking it before phase 8 (visual polish) to get a distinctive aesthetic pass.

## 6. Theming

- Tailwind `darkMode: 'class'`.
- Theme detector reads `prefers-color-scheme`, sets `<html class="dark">` accordingly.
- Manual toggle: `system | light | dark` (3-state), persisted to `localStorage.theme`.
- All colors expressed via CSS variables (`--bg`, `--fg`, `--surface`, `--accent`, ...) defined per theme in a single `theme.css`.

## 7. Key UX details (from spec)

### Topic tree (host)

- Inline edit (double-click title).
- Drag to reorder + indent (one-level at a time via Tab/Shift-Tab when editing).
- "Active" is a single radio across all topics. Clicking "set active" on a new topic auto-marks the previously-active as done.
- Done topics: muted color + checkmark; can re-open by clicking the check.
- Keyboard shortcut on host: `j` / `k` advances to next/prev pending topic + sets active (presenter mode).

### Q&A

- List defaults to **chronological, newest at bottom**.
- "Resort by votes" button toggles between chronological and `votes desc, createdAt asc`.
- New questions append; "↑ New questions" pill appears if the user has scrolled away from the bottom.
- **Autoscroll lock**: ScrollArea tracks `isAtBottom`; if user scrolls up, lock engages (no autoscroll). If they scroll back to within 50px of bottom, lock auto-disengages.
- Jump-to-top and jump-to-bottom buttons in the corner.
- Anonymous checkbox in composer. Tooltip: "Your name won't show on this question."
- Vote button: heart/up-arrow icon + count. Disabled state if already voted. Click again to unvote.
- Host sees an "Answered" toggle on each question.

### Whiteboards

- "Follow host" is a toggle in the BoardTabs strip; if on, the local view's focused board mirrors the server's `focusedBoardId`.
- If a guest turns Follow off and views board X, then host switches focused board to Y, guest stays on X.
- Click pings: 600ms expanding/fading ring at click coordinate with the clicker's display name floating above for 1.2s.
- Cursors: throttled remote cursor positions with display name label; only show for clients active in last 5s.

### Moderation (host)

- Per-guest menu: rename, mute (can't ask/vote), kick (close ws + add to room blocklist).
- Per-question menu: delete, mark answered, move to topic (assign `topic_id`).

### Presence

- Top-right shows live count + a hover-card listing names. Anonymous-only behavior: even if a user is anonymous for a question, their presence name is still visible (anonymity is per-question, not session).

## 8. Frontend file layout (web/src/)

```
src/
├── main.tsx
├── App.tsx
├── routes/
│   ├── Landing.tsx
│   ├── Rooms.tsx
│   ├── RoomEntry.tsx           # name prompt
│   ├── Session.tsx             # the three-column shell
│   └── HostClaim.tsx           # ?admin=<token> handler
├── components/
│   ├── ThemeToggle.tsx
│   ├── Topbar.tsx
│   ├── Avatar.tsx
│   └── ...
├── topictree/
├── qa/
├── whiteboard/
│   ├── PenBoard.tsx
│   ├── ExcalidrawBoard.tsx
│   ├── CursorLayer.tsx
│   ├── ClickPing.tsx
│   └── tools/
├── ws/
│   ├── client.ts               # WS reconnect, hello/welcome, dispatch
│   ├── messages.ts             # client→server send helpers
│   └── reducer.ts              # server→client store updates
├── store/
│   ├── index.ts                # zustand store factory
│   ├── topics.ts
│   ├── questions.ts
│   ├── boards.ts
│   ├── me.ts
│   └── ui.ts
├── proto/
│   └── generated.ts            # produced from server's ts-rs
├── lib/
│   ├── idb.ts                  # IndexedDB room registry
│   ├── id.ts                   # uuid + short id generators
│   └── throttle.ts
└── theme/
    └── theme.css
```
