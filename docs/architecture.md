# Architecture

## System Overview

topic-tree-with-qav is a single Rust binary serving both a static React frontend and a WebSocket/HTTP API. All state lives in a single SQLite database on one host.

```
┌─────────────────────────────────────────────────────┐
│                     Clients                          │
│  Browser ─── WebSocket + HTTP ──── Admin token     │
└──────────┬──────────────────────────┬───────────────┘
           │                          │
           ▼                          ▼
┌──────────────────────────────────────────────────────┐
│          Rust Binary (Axum + Tokio)                  │
│                                                       │
│  ┌─────────────┐   ┌─────────────┐  ┌────────────┐ │
│  │  HTTP API    │   │  WebSocket  │  │  Static    │ │
│  │  /api/*      │   │  /ws        │  │  Assets    │ │
│  └──────┬───────┘   └──────┬──────┘  └─────┬──────┘ │
│         │                   │               │         │
│  ┌──────▼───────────────────▼───────────────▼──────┐ │
│  │              State Manager (in-memory)           │ │
│  │                                                       │
│  │  rooms/ topics/ questions/ presence/ boards        │ │
│  └──────────────────────┬────────────────────────────┘ │
│                         │                              │
│  ┌──────────────────────▼────────────────────────────┐ │
│  │              SQLite (WAL, single-writer)           │ │
│  │              DATABASE_PATH=/data/app.db            │ │
│  └───────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────┘
```

## Frontend Stack

- **Vite** + **React** + **TypeScript**
- **Tailwind CSS** + **shadcn/ui** for styling
- **Zustand** for client-side state
- **perfect-freehand** for pen whiteboard strokes
- **@excalidraw/excalidraw** for Excalidraw boards
- **react-router-dom** for routing

Built assets are embedded into the Rust binary via `rust-embed`.

## Backend Stack

- **Rust** + **Axum** on **Tokio** (multi-threaded runtime)
- **tokio-tungstenite** for WebSocket connections
- **rusqlite** (bundled) + **r2d2** connection pool + **refinery** for migrations
- **tracing** for structured logging

## Protocol

All real-time communication uses raw WebSockets with JSON envelopes. See the protocol reference in `.plan/2026-05-24-amber-falcon/protocol.md`.

Message types:

- `Hello` — initial handshake with role (`host` or `guest`) and credentials
- `Event` — server-broadcast events (topic added, question submitted, cursor moved, etc.)
- `Intent` — client requests (add topic, submit question, vote, etc.)

## Data Model

Single SQLite database with tables for:

- `rooms` — room metadata and admin token hash
- `topics` — hierarchical topic tree nodes
- `questions` — Q&A entries with vote counts
- `votes` — deduplication of votes per guest
- `guests` — guest presence and display names
- `pen_boards` / `pen_strokes` — pen whiteboard content
- `excalidraw_boards` — Excalidraw board snapshots
- `raise_hand_queue` — ordered guest queue

## Identity

- **Host**: random `adminToken` generated server-side, argon2-hashed in the DB, stored in browser IndexedDB. Passed via `?admin=` URL param, stripped client-side within 50ms.
- **Guest**: self-issued `guestId` (UUIDv4) in `localStorage` + chosen display name. Verified once at `Hello`; role cached per connection.

## Deployment

- **Railway**: single service with a volume at `/data`. Single-instance by design.
- **Self-host**: any server with Docker or a Rust toolchain.
- **GitHub Pages**: docs site only (VitePress SSG). App binary must be hosted separately.
