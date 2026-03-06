# Agora Parallel Work Plan

Multi-instance development coordination. Run tasks wave by wave. Each wave's tasks are independent and can run in parallel.

---

## Instance Assignments

| Instance | Focus Area | Primary Crate(s) |
|----------|------------|------------------|
| **Instance 1** | Frontend / UX | `agora-app` (SvelteKit/Tauri) |
| **Instance 2** | Backend / Protocol | `agora-server`, `agora-core` |
| **Instance 3** | Core / Integration | `agora-core`, `agora-crypto`, `agora-cli` |

---

## Wave 1: Dogfood Launch Blockers

**Status:** 🔲 Ready to start  
**Goal:** Verify core functionality before internal adoption

| Instance | Task | Files / Areas | Deliverable |
|----------|------|---------------|-------------|
| **Instance 1** | Verify image rendering E2E | `agora-app/src/lib/components/MessageContent.svelte`, decryption path | Test matrix: unencrypted room, encrypted 1:1, encrypted group — all render correctly |
| **Instance 2** | Document E2E limitations | `README.md` or `docs/E2E.md` | Explain why Agora E2E (`m.agora.pairwise.v1`) is NOT compatible with Element/Olm/Megolm |

**Acceptance Criteria:**
- [ ] Images display in all room types without manual refresh
- [ ] Clear documentation exists warning users not to expect Element compatibility in encrypted rooms

---

## Wave 2: Core UX Features

**Status:** 🔲 Blocked on Wave 1  
**Goal:** Reach parity with modern chat platforms

| Instance | Task | Files / Areas | Notes |
|----------|------|---------------|-------|
| **Instance 1** | **Emoji reactions** | `agora-core/src/events/reaction.rs` (new), `agora-app/src/lib/components/Reactions.svelte` (new), message hover UI | Matrix `m.reaction` event type; aggregate by emoji; show count + users |
| **Instance 2** | **User avatars** | `agora-server/src/routes/media.rs` (extend), `agora-server/src/routes/profile.rs`, `agora-app/src/lib/components/Avatar.svelte` | Upload endpoint exists; wire up profile avatar_url; display in message list |
| **Instance 3** | **Presence/status** | `agora-core/src/events/presence.rs` (new), `agora-server/src/routes/presence.rs` (new), sync response | `m.presence` with `online`, `unavailable`, `offline`; heartbeat every 30s; indicator in user list |

**Acceptance Criteria:**
- [ ] Can add emoji reaction to any message; persists across reload
- [ ] Can upload avatar; visible in messages and member list
- [ ] Presence changes propagate; UI shows correct status

---

## Wave 3: Advanced Features

**Status:** 🔲 Blocked on Wave 2  
**Goal:** Differentiating features

| Instance | Task | Files / Areas | Notes |
|----------|------|---------------|-------|
| **Instance 1** | **Video inline player** | `agora-app/src/lib/components/MessageContent.svelte` | Detect video mimetype; render `<video>` element; lazy load |
| **Instance 2** | **Voice chat (WebRTC)** | `agora-core/src/events/call.rs` (new), `agora-server/src/signaling.rs` (new) | `m.call.invite`, `m.call.candidates`, `m.call.answer`, `m.call.hangup`; signaling only (no TURN yet) |
| **Instance 3** | **Room moderation** | `agora-core/src/room/power_levels.rs` (extend), `agora-server/src/routes/rooms.rs` | Kick, ban, unban endpoints; enforce power levels; add to TUI |

**Acceptance Criteria:**
- [ ] MP4/WebM videos play inline without download
- [ ] 1:1 voice call connects; audio flows
- [ ] Admin can kick/ban; power levels respected

---

## Wave 4: Infrastructure

**Status:** 🔲 Blocked on Wave 3  
**Goal:** Production readiness features

| Instance | Task | Files / Areas | Notes |
|----------|------|---------------|-------|
| **Instance 1** | **Push notifications** | `agora-app/src-tauri/` (Tauri notifications), `agora-server/src/push/` | WebPush integration; notification on mention/PM; settings panel |
| **Instance 2** | **Full-text search** | `agora-server/src/routes/search.rs` (new), SQLite FTS5 or meilisearch | `/search` endpoint; index room history; search by sender, content, date |
| **Instance 3** | **Read receipts** | `agora-server/src/routes/receipts.rs` (new), sync response | `m.read` private receipt; update sync to include; show unread counts |

**Acceptance Criteria:**
- [ ] Desktop notification fires on mention
- [ ] Search returns relevant results in <500ms
- [ ] Read receipts sync; unread badge clears on read

---

## Wave 5: AMP Phase 1 — LAN Mesh

**Status:** 🔲 Blocked on Wave 4  
**Goal:** First step toward Agora Mesh Protocol

| Instance | Task | Files / Areas | Notes |
|----------|------|---------------|-------|
| **Instance 1** | **mDNS discovery** | New `agora-mesh/src/discovery.rs` | Broadcast `_agora._tcp.local`; query for peers; emit peer-up/peer-down events |
| **Instance 2** | **QUIC direct transport** | New `agora-mesh/src/transport.rs` | QUIC over IPv4/IPv6; handshake with existing identity keys; basic message relay |
| **Instance 3** | **`agora connect --local`** | `agora-cli/src/main.rs`, `agora-cli/src/mesh.rs` | New flag; bypass server; peer-to-peer mode; use mDNS + QUIC |

**Acceptance Criteria:**
- [ ] Two laptops on same WiFi discover each other
- [ ] Can send message without server
- [ ] CLI flag works; mesh mode indicator in UI

---

## Shared Resources & Conventions

### Before Starting Any Task
1. Check this file for current wave status
2. Claim your task by adding your name/instance ID in the table
3. Run tests: `cargo test --workspace`
4. Create feature branch: `git checkout -b wave{N}/{task-name}`

### Event Type Naming
- Use Matrix spec names where applicable (`m.reaction`, `m.presence`)
- Use Agora prefix for extensions (`agora.tool_call`, `agora.sigchain_proof`)

### Database Migrations
- Place in `agora-server/migrations/`
- Name: `{timestamp}_{description}.sql`
- Run: `cargo sqlx migrate run`

### Testing
- Unit tests in same file as code
- Integration tests in `tests/` directory per crate
- E2E tests: manual verification with two clients

---

## Current Wave Status

| Wave | Status | Blocked By |
|------|--------|------------|
| Wave 1 | 🟡 Ready | — |
| Wave 2 | 🔲 Waiting | Wave 1 completion |
| Wave 3 | 🔲 Waiting | Wave 2 completion |
| Wave 4 | 🔲 Waiting | Wave 3 completion |
| Wave 5 | 🔲 Waiting | Wave 4 completion |

---

## Task Claim Log

| Wave | Task | Claimed By | Branch | Status |
|------|------|------------|--------|--------|
| 1 | Image rendering E2E | | | 🔲 Todo |
| 1 | E2E docs | | | 🔲 Todo |
| 2 | Emoji reactions | | | 🔲 Todo |
| 2 | User avatars | | | 🔲 Todo |
| 2 | Presence/status | | | 🔲 Todo |

*Update this table as tasks are claimed and completed.*

---

## Questions?

Check `README.md` for architecture overview.  
Check `ROADMAP.md` for AMP long-term vision.  
Check `TODO.md` for the canonical backlog.

