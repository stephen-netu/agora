#!/usr/bin/env bash
# start-server.sh — Start the Agora homeserver
# Runs on the default address http://127.0.0.1:8008
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "Starting Agora server..."
echo "Address: http://127.0.0.1:8008"
echo "Press Ctrl+C to stop."
echo ""

exec cargo run -p agora-server
