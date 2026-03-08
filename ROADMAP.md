# Agora Status

> **Canonical task tracking**: `~/Projects/.sovereign/tasks/world-tree.yaml`

All Agora tasks are tracked in the World Tree. This file exists only for backward compatibility.

## Current Status

Almost all Agora tasks are **complete**. Only one task remains pending:

| Task ID | Description | Status |
|---------|-------------|--------|
| wt-026 | Migrate agora-crypto::AgentId to sovereign-sdk::AgentId | pending |

## What's Implemented

- ✅ agora-p2p with QUIC transport
- ✅ agora-p2p with Yggdrasil transport adapter
- ✅ agora-crypto (Ed25519 identity, sigchain, Double Ratchet E2E)
- ✅ agora-server (Matrix-compatible homeserver)
- ✅ agora-app (Tauri + Svelte desktop client)
- ✅ agora-cli (CLI + TUI)
- ✅ Atelier integration with agora-p2p
- ✅ sovereign-client library
- ✅ Phases 1–10 protocol extensions (collaboration, fuel, disputes, anchoring, ZK, credits)

## Running Agora

```bash
# Start the server
cargo run -p agora-server

# Or use the desktop app (requires server)
cargo run -p agora-app
```
