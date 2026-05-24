# topic-tree-with-qav

Real-time host-audience interaction: topic tree, Q&A with voting, smooth whiteboards, raise-hand.

**Live:** [Railway (production)](https://topic-tree-with-qav.up.railway.app) · [GitHub Pages (docs)](https://clankercode.github.io/topic-tree-with-qav/)

## Quick Start

```bash
# install deps
just setup

# run dev (vite + rust server)
just dev
```

## Stack

- **Frontend:** React + TypeScript + Tailwind + shadcn/ui + Zustand
- **Backend:** Rust + Axum + SQLite (WAL)
- **Realtime:** raw WebSockets
- **Whiteboards:** perfect-freehand (pen) + Excalidraw
- **Hosting:** Railway (production) · GitHub Pages (docs)

## Features

- Hierarchical topic tree driven by the host
- Q&A with anonymous voting
- Freehand pen whiteboard + Excalidraw board
- Live cursors + click pings
- Raise-hand queue for structured participation

## Docs

Full documentation at [clankercode.github.io/topic-tree-with-qav/](https://clankercode.github.io/topic-tree-with-qav/)

## License

MIT
