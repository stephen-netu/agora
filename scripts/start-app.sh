#!/usr/bin/env bash
# start-app.sh — Start the Agora desktop application (Tauri)
#
# Builds the frontend if it hasn't been built yet, then launches the app.
# For a full rebuild of the frontend use: build-all.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONTEND_DIR="$REPO_ROOT/agora-app/frontend"
FRONTEND_BUILD="$FRONTEND_DIR/build"

cd "$REPO_ROOT"

# Build the frontend if the build directory doesn't exist
if [ ! -d "$FRONTEND_BUILD" ]; then
    echo "Frontend build not found — building now..."
    if [ ! -d "$FRONTEND_DIR/node_modules" ]; then
        echo "Installing frontend dependencies..."
        npm install --prefix "$FRONTEND_DIR"
    fi
    npm run build --prefix "$FRONTEND_DIR"
    echo ""
fi

echo "Starting Agora desktop app..."
echo "Press Ctrl+C to stop."
echo ""

exec cargo run -p agora-app
