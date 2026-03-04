#!/usr/bin/env bash
# build-all.sh — Full release build of Agora (server + CLI + desktop app)
#
# Produces optimised binaries in target/release/:
#   agora-server   — homeserver
#   agora-cli      — command-line client
#   agora-app      — desktop application
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONTEND_DIR="$REPO_ROOT/agora-app/frontend"

cd "$REPO_ROOT"

# ── Frontend ──────────────────────────────────────────────────────────────────
echo "==> Installing frontend dependencies..."
npm install --prefix "$FRONTEND_DIR"

echo "==> Building frontend..."
npm run build --prefix "$FRONTEND_DIR"

# ── Rust workspace ────────────────────────────────────────────────────────────
echo ""
echo "==> Building Rust workspace (release)..."
cargo build --release -p agora-server -p agora-cli -p agora-app

echo ""
echo "Build complete."
echo ""
echo "Binaries:"
echo "  $(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["target_directory"])' 2>/dev/null || echo "$REPO_ROOT/target")/release/agora-server"
echo "  $(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["target_directory"])' 2>/dev/null || echo "$REPO_ROOT/target")/release/agora-cli"
echo "  $(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["target_directory"])' 2>/dev/null || echo "$REPO_ROOT/target")/release/agora-app"
echo ""
echo "Run scripts/start-server.sh and scripts/start-app.sh to launch."
