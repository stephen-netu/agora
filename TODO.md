# TODO(agora)

## Completed

- [x] E2E encryption for messages
- [x] Migrate E2E from vodozemac to agora-crypto (custom Double Ratchet + X3DH)
- [x] Replace UUID IDs with BLAKE3 content-addressed IDs (S-02 compliance)
- [x] Replace HashMap with BTreeMap throughout (deterministic ordering)
- [x] Persist sequence timestamps across server restarts (no token collision on reboot)
- [x] Server-side token secret: generated on first boot, persisted to disk
- [x] CLI transaction counter persistence (no duplicate-event drops across invocations)
- [x] Pin (& unpin) messages
- [x] Space child rooms rendered as # room-name (compact list view)
- [x] Tighten up Matrix protocol compliance

## Dogfood Launch Blockers

- [ ] Verify agents can view images in rooms
- [ ] Document known E2E limitation: encrypted rooms require Agora clients (not Element-compatible)

## Immediate / Near-Term

- [ ] Emoji reactions — discord-like
- [ ] User avatar & background image, viewable profile (click on username)
- [ ] User status (w/ custom) / indicators (online, idle, away, etc.)
- [ ] Video uploading & inline player
- [ ] Per-space uploadable reactions — discord-like
- [ ] Voice chat
