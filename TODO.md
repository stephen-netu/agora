# TODO(agora)

## Completed

### Core Infrastructure
- [x] E2E encryption for messages
- [x] Migrate E2E from vodozemac to agora-crypto (custom Double Ratchet + X3DH)
- [x] Replace UUID IDs with BLAKE3 content-addressed IDs (S-02 compliance)
- [x] Replace HashMap with BTreeMap throughout (deterministic ordering)
- [x] Persist sequence timestamps across server restarts (no token collision on reboot)
- [x] Server-side token secret: generated on first boot, persisted to disk
- [x] CLI transaction counter persistence (no duplicate-event drops across invocations)
- [x] Tighten up Matrix protocol compliance

### Messaging & Rooms
- [x] Pin (& unpin) messages
- [x] Space child rooms rendered as # room-name (compact list view)
- [x] File upload and inline media display (images, files, audio, video)
- [x] Invite user to room (server endpoint + frontend modal)
- [x] Receive and accept/decline room invitations (frontend)
- [x] Delete room (creator only)
- [x] Leave room

### User Directory
- [x] `POST /v3/user_directory/search` — search users by ID or display name
- [x] Live search-as-you-type in the Invite User modal (300ms debounce)
- [x] `api.searchUsers()` in the frontend API client

### Sigchain Behavioral Ledger
- [x] `agora-crypto` sigchain: append-only, hash-linked, Ed25519-signed action log
- [x] Genesis, Action, Checkpoint, TrustTransition, and Refusal link types
- [x] S-05 loop detection: `Refusal` link appended if agent ID already in correlation path
- [x] Sigchain integration in CLI (`send` command and interactive TUI)
- [x] Sigchain integration in desktop app (`handleSend`, `handleFileUpload`)
- [x] `agora agent-id` — CLI command to display this device's sigchain identity
- [x] Agent ID displayed in Settings → Encryption tab alongside device fingerprint
- [x] Sigchain badges (⛓ #N) on incoming messages in both TUI and web app
- [x] `sigchain_proof: { seqno, agent_id }` embedded in outgoing event content
- [x] Sigchain API on server: `PUT /_agora/sigchain/{agentId}`, `GET`, `/verify`

### Developer Experience
- [x] Cross-platform startup scripts (`scripts/`): `.sh` (Mac/Linux), `.bat` and `.ps1` (Windows)
  - `start-server` — launch homeserver
  - `start-tui` — launch interactive TUI (accepts any CLI subcommand as args)
  - `start-app` — build frontend if needed, launch desktop app
  - `build-all` — full release build (frontend + all Rust crates)
- [x] Frontend build (`agora-app/frontend/build/`) — Tauri desktop app fully runnable

---

## Dogfood Launch Blockers

- [ ] Verify image rendering works end-to-end in encrypted and unencrypted rooms
- [x] Document known E2E limitation: encrypted rooms require Agora clients (not Element-compatible)

---

## Immediate / Near-Term

- [ ] Emoji reactions — Discord-style, per-message
- [ ] User avatar upload & display, viewable profile on username click
- [ ] User status (online / idle / away / custom) with presence indicators
- [ ] Video inline player (upload exists; playback UI not yet implemented)
- [ ] Per-space custom uploadable reactions — Discord-style
- [ ] Voice chat

---

## Longer-Term (see ROADMAP.md for detail)

- [ ] Read receipts
- [ ] Room moderation: kick, ban, unban, power level enforcement
- [ ] Presence (`/presence/{userId}/status`, sync integration)
- [ ] Account data (global and per-room)
- [ ] Server-side sync filters
- [ ] E2E enhancements: device lists in sync, key backup, cross-signing, encrypted attachments
- [ ] Full-text message search (`POST /search`)
- [ ] Federation (Matrix server-to-server API)
- [ ] Push notifications
- [ ] Authentication enhancements: UIAA, password change, SSO, refresh tokens
- [ ] Media thumbnailing and URL previews
