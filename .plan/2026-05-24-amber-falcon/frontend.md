# Frontend

## 1. Routes

| Path | Purpose |
|---|---|
| `/` | Landing — "Create room" CTA (`POST /api/rooms`); shows recently created/joined rooms from IndexedDB |
| `/r/:roomId` | Guest entry — name prompt then session view |
| `/r/:roomId/host` | Host view (gated by IndexedDB-stored `adminToken`; redirects to landing if missing) |
| `/r/:roomId?admin=<token>` | First-time host claim — strips query within 50ms, saves token to IndexedDB, redirects to `/r/:roomId/host` |
| `/rooms` | Dashboard of all rooms this device knows about (host + joined-as-guest) |
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

## 3. Component inventory (routed tree, canonical names)

```
RootLayout
├── Landing                       (route "/")
│   ├── CreateRoomCTA
│   ├── RecentRoomsList           (IndexedDB-backed)
│   └── ThemeToggle
├── RoomsDashboard                (route "/rooms")
│   └── RoomCard
├── HostClaim                     (route "/r/:id?admin=...")
├── RoomEntry                     (route "/r/:id"  guest pre-join)
│   ├── NameInput
│   └── JoinButton
└── SessionLayout                 (route "/r/:id" + "/r/:id/host")
    ├── Topbar
    │   ├── RoomTitle
    │   ├── PresenceIndicator → PresenceHoverCard
    │   ├── AdminBanner          (host only, copy join + copy admin)
    │   ├── RaiseHandButton      (guest only)
    │   ├── ThemeToggle
    │   └── HostMenu             (host only)
    ├── ColumnSplitter            (desktop ≥900px)
    │   ├── TopicTree
    │   │   ├── TopicNode
    │   │   │   ├── TopicStatusIndicator
    │   │   │   └── TopicEditPopover (host only)
    │   │   └── ActiveTopicBadge
    │   ├── BoardArea
    │   │   ├── BoardTabs
    │   │   │   └── FollowHostToggle (guest only)
    │   │   ├── BoardCanvas
    │   │   │   ├── PenBoard
    │   │   │   │   ├── PenCanvas
    │   │   │   │   ├── PenTextLayer
    │   │   │   │   └── PenToolPalette (host only)
    │   │   │   └── ExcalidrawBoard
    │   │   ├── CursorLayer
    │   │   └── ClickPingLayer
    │   └── QAPanel
    │       ├── QuestionComposer
    │       ├── QuestionList
    │       │   └── QuestionItem
    │       │       └── VoteButton
    │       ├── SortToggle
    │       ├── AutoscrollLock (logic only, no UI)
    │       └── JumpButtons
    ├── MobileTabBar              (mobile <900px alternative to ColumnSplitter)
    └── HandsQueuePanel           (host only; slides in from right when ≥1 hand)
        └── HandQueueEntry
            ├── CallOnButton
            └── DismissButton
```

**Shared atomics**: `Avatar`, `Toast`, `Banner`, `EmptyState`, `LoadingDots`, `ConfirmDialog`.

(`ModerationPanel` and `RoomSettings` are deliberately out of v1 — moderation lives in per-presence menus on the `PresenceHoverCard`; settings can be added later via `rooms.settings_json`.)

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

- Inline edit (double-click title) or Rename button.
- Add subtopic per node; drag to reorder within sibling lists; drop on a node to reparent as last child; Tab/Shift-Tab indent/outdent while editing.
- Collapsible nodes (local browser state only); collapsed rows show subtopic count.
- Max nesting depth: 10 levels (server-enforced on add/move/import).
- "Active" is a single radio across all topics. Clicking "set active" on a new topic auto-marks the previously-active as done.
- Done topics: muted color + checkmark; can re-open by clicking the check.
- Keyboard shortcut on host: `j` / `k` advances to next/prev pending topic + sets active (presenter mode).

### Topic tree (guest)

- Vote button (+1, toggle retract) on each topic; same dedup/rate-limit pattern as Q&A.
- Done topics remain voteable with faded styling.
- Vote counts are display-only; host controls order via drag/`ord`.

### Q&A

- List defaults to **chronological, newest at bottom**.
- "Resort by votes" button toggles between chronological and `votes desc, createdAt asc`.
- New questions append; "↑ New questions" pill appears if the user has scrolled away from the bottom.
- **Autoscroll lock**: ScrollArea tracks `isAtBottom`; if user scrolls up, lock engages (no autoscroll). If they scroll back to within 50px of bottom, lock auto-disengages.
- Jump-to-top and jump-to-bottom buttons fixed in the corner of `QAPanel`.
- Anonymous checkbox in composer. Tooltip: "Your name won't show on this question."
- Vote button: heart/up-arrow icon + count. Disabled state if already voted. Click again to unvote. Client tracks own votes from `Welcome.myVotes` + each `VoteUpdated.voterGuestId`.
- **Answered questions are visually demoted** (muted text, strikethrough, faded vote count) and **sink to the bottom of the chronological list** (sorted by `(answered asc, createdAt asc)` so unanswered always above answered). They are not hidden — visible for context. Host's "Answered" toggle is a per-row button that flips the bool.

### Whiteboards

- "Follow host" is a toggle in the BoardTabs strip; if on, the local view's focused board mirrors the server's `focusedBoardId`.
- If a guest turns Follow off and views board X, then host switches focused board to Y, guest stays on X.
- Click pings: 600ms expanding/fading ring at click coordinate with the clicker's display name floating above for 1.2s.
- Cursors: throttled remote cursor positions with display name label; only show for clients active in last 5s.

### Moderation (host)

- **Per-guest menu** (from `PresenceHoverCard`): mute / unmute (toggle; muted guests can't `SubmitQuestion`/`VoteQuestion`/`VoteTopic`/`RaiseHand`), kick (close ws + add to room blocklist). No host-side rename — guests own their display name via `SetDisplayName`.
- **Per-question menu** (host only, on `QuestionItem`): delete, mark answered (toggle), **promote to topic** (atomically creates a new topic-tree node from the question and deletes the question via `PromoteQuestionToTopic`).

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
