# Host Guide

## Creating a Room

1. Open the app and click **Create room**.
2. You are redirected to the host session URL containing your `admin` token.
3. Copy and share the guest URL (without the `admin` token) with your audience.

::: warning Save the Admin Link
The admin token in the URL is stored in your browser's IndexedDB. If you clear site data, you will lose host access for that room.
:::

## Topic Tree

The topic tree is the backbone of the session. As a host:

- **Add topics** using the "+" button in the tree panel.
- **Reorder** topics by dragging them.
- **Activate** a topic by clicking it — active topic is highlighted for all guests.
- Guests see the tree structure and the currently active topic in real time.

## Q&A Panel

The Q&A panel shows submitted questions sorted by vote count:

- **Vote** on questions to move the most relevant ones to the top.
- **Answer** a question by clicking it — it becomes the "answering now" item.
- Questions can be submitted anonymously by guests; moderation still works server-side.

## Whiteboards

Two whiteboard modes are available:

### Pen Board

- Host creates a **Pen Board** via "Create Board" → "Pen".
- Draw with freehand strokes using `perfect-freehand`.
- All guests see the board in real time.

### Excalidraw Board

- Host creates an **Excalidraw Board** via "Create Board" → "Excalidraw".
- Full Excalidraw editor with shapes, text, and drawing tools.
- `viewModeEnabled` is display-only; the server still enforces admin-only edits.

## Raise Hand

Guests can raise their hand to request to speak. The host sees a queue:

- Click **Raise Hand** to add yourself to the queue.
- The host can dismiss guests from the queue.
- Raise hand state is visible to all participants.

## Moderation

- **Kick**: Host can disconnect any guest session.
- **Mute**: Rate limits enforce per-IP deduplication of votes and messages.
- Anonymous questions are stored with `guest_id` server-side even if the display name is hidden.

## Admin Banner

When accessing a room as host, a banner indicates your admin status. If the banner is missing, you may not have valid admin credentials for that room.

## Ending a Session

Close the browser tab or navigate away. The room persists in the database until you navigate to the host dashboard and claim the room. Rooms are not automatically deleted (see [Self-host guide](../deployment/self-host.md) for cleanup).
