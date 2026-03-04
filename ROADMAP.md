# Agora Roadmap

## Current State

Agora is a self-hosted, privacy-first communications platform implementing a substantial subset of the Matrix Client-Server API (v1.11). The server runs as a single binary with SQLite storage. All three clients (desktop app, web frontend via Tauri, interactive TUI) are fully operational.

### What ships today

**Protocol**: Authentication, rooms, spaces, messaging, file media upload/download, E2E encryption, profiles, typing indicators, devices, room directory, user directory search, pinned messages, room invitations (send and receive), room deletion, space hierarchy.

**Cryptography** (`agora-crypto`): Built on audited primitives — X25519, Ed25519, ChaCha20-Poly1305, BLAKE3, HKDF. All IDs are BLAKE3 content-addressed; timestamps are deterministic sequence counters that survive restarts without collision. E2E uses Signal-spec Double Ratchet + X3DH (`m.agora.pairwise.v1`) and sender-key group sessions (`m.agora.group.v1`). These are internal algorithm identifiers — encrypted rooms are only readable by Agora clients, not Element or other standard Matrix clients.

**Sigchain Behavioral Ledger**: Every outgoing message and file send is signed and appended to the sender's local sigchain — an append-only, BLAKE3-hash-linked, Ed25519-signed action log. Each signed action is published to the server and embedded in the Matrix event as `sigchain_proof: { seqno, agent_id }`. Recipients see a ⛓ #N badge on verified messages. S-05 loop detection prevents recursive agent amplification by appending a `Refusal` link if the sender's Agent ID is already present in the correlation path. Agent IDs are displayed in the Settings → Encryption tab and via `agora agent-id` in the CLI.

**Clients**:
- **Desktop app** (Tauri + Svelte 5): full messaging, E2EE, file uploads, pinned messages, invite modal with live user search, sigchain badges, theme switcher (dark/light/seraphim), settings panel.
- **Interactive TUI** (`agora connect`): ratatui-based terminal client with room switching, scrollback, sigchain-signed sending.
- **CLI** (`agora-cli`): `register`, `login`, `logout`, `rooms`, `spaces`, `send`, `messages`, `upload`, `download`, `connect`, `agent-id`.

**Startup scripts** (`scripts/`): `.sh` for Mac/Linux, `.bat` and `.ps1` for Windows — no hardcoded paths, work from any location.

---

## Remaining Items for Element/Standard Client Compatibility

The following features would be needed for full compatibility with standard Matrix clients like Element, FluffyChat, etc.

### Filters

- `POST /user/{userId}/filter` — create server-side filter
- `GET /user/{userId}/filter/{filterId}` — retrieve filter
- `filter` parameter on `/sync` — apply filter to sync response
- `filter` parameter on `/messages` — apply RoomEventFilter

### Room Moderation

- `POST /rooms/{roomId}/kick` — kick a user from a room
- `POST /rooms/{roomId}/ban` — ban a user from a room
- `POST /rooms/{roomId}/unban` — unban a user
- Power level enforcement on all state-changing operations
- `m.room.power_levels` state event processing and validation

### Presence

- `PUT /presence/{userId}/status` — set presence status (online/offline/unavailable)
- `GET /presence/{userId}/status` — get presence status
- `set_presence` query parameter on `/sync`
- `presence` section in sync response with `m.presence` events
- Automatic idle/offline detection based on inactivity

### Read Receipts

- `POST /rooms/{roomId}/receipt/{receiptType}/{eventId}` — send read receipt
- Include receipts in `ephemeral.events` section of sync response
- `m.read` and `m.read.private` receipt types
- Fully-read markers (`m.fully_read` account data)

### Account Data

- `PUT /user/{userId}/account_data/{type}` — set global account data
- `GET /user/{userId}/account_data/{type}` — get global account data
- `PUT /user/{userId}/rooms/{roomId}/account_data/{type}` — per-room account data
- `GET /user/{userId}/rooms/{roomId}/account_data/{type}` — get per-room account data
- Include `account_data` section in sync response (global and per-room)

### Room Versioning

- Room version field in `m.room.create` content
- `POST /rooms/{roomId}/upgrade` — upgrade room to a new version
- Proper handling of room version capabilities

### E2E Encryption Enhancements

- `device_lists` section in sync response (changed/left device tracking)
- Device list change notifications when users join/leave rooms
- Cross-signing (master/self-signing/user-signing keys)
- Key backup (`/room_keys/` endpoints)
- Key sharing (`m.room_key_request`, `m.forwarded_room_key`)
- Encrypted attachments (AES-CTR file encryption)
- Verification (SAS, QR code)

### Federation (Server-to-Server API)

- `/_matrix/federation/v1/` endpoints
- Server key management and signing
- Event authorization and state resolution
- Backfill from remote servers
- Room joins via federation

### Authentication Enhancements

- User-Interactive Authentication (UIAA) for `/register` and sensitive operations
- `m.login.token` login type
- SSO/OAuth2 login flow
- Refresh tokens (`refresh_token` in login response)
- Password change (`POST /account/password`)

### Push Notifications

- `GET /pushers` — list push notification targets
- `POST /set/pusher` — configure push notifications
- `GET /pushrules/` — get push rules
- `PUT /pushrules/` — set push rules
- Push gateway integration

### Content Repository Enhancements

- `GET /media/v3/thumbnail/{serverName}/{mediaId}` — thumbnailing
- Content-Type validation
- Media quarantine
- URL previews (`GET /media/v3/preview_url`)

### Room Alias Resolution in /join

- Resolve `#alias:server` to room_id in the `/join/{roomIdOrAlias}` endpoint
- Support `server_name` parameter for joining via alias

### Full-Text Search

- `POST /search` — full-text message search
- Search by room, sender, and content

### Third-Party Networks

- `GET /thirdparty/protocols` — list bridged protocols
- Application service (bridge) registration

---

## Agora-Native Feature Backlog

Features beyond Matrix compatibility — Agora-specific enhancements on the near and medium horizon.

### Near-Term
- Emoji reactions (Discord-style, per-message)
- User avatars and viewable profiles (click username to view)
- User status and presence indicators (online/idle/away/custom)
- Video inline playback (upload already works; player UI pending)
- Per-space custom uploadable reactions

### Medium-Term
- Voice chat (WebRTC or equivalent)
- Sigchain verification UI — click ⛓ badge to inspect and verify the full action chain
- Thread / reply support (Matrix `m.relates_to` with `m.thread`)
- Markdown rendering in messages
- Notification badges on room list (unread counts)
- Message editing and deletion (redaction)

### Longer-Term
- Sigchain-attested agent actions: LLM agents that act on behalf of users with full auditability
- Federated sigchain verification across homeservers
- Mobile clients (iOS/Android via Tauri Mobile or React Native)

---

## Feature Priorities

1. **High** — Read receipts, moderation (kick/ban), account data, filters
2. **Medium** — Presence, emoji reactions, user profiles/avatars, E2E enhancements (device lists, key backup, cross-signing)
3. **Low** — Federation, push notifications, SSO, third-party networks, full-text search
