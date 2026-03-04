#!/usr/bin/env bash
# start-tui.sh — Start the Agora interactive terminal UI
#
# Usage:
#   ./start-tui.sh                         # connect (TUI)
#   ./start-tui.sh login -u alice -p pass  # any agora-cli subcommand
#   ./start-tui.sh --help
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# If no args given, default to the interactive TUI
if [ $# -eq 0 ]; then
    exec cargo run -p agora-cli -- connect
else
    exec cargo run -p agora-cli -- "$@"
fi
