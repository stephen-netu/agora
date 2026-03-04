# start-tui.ps1 — Start the Agora interactive terminal UI
#
# Usage:
#   .\start-tui.ps1                          # connect (TUI)
#   .\start-tui.ps1 login -u alice -p pass   # any agora-cli subcommand
#   .\start-tui.ps1 --help
#
# If execution policy blocks this, run once:
#   Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

if ($args.Count -eq 0) {
    & cargo run -p agora-cli -- connect
} else {
    & cargo run -p agora-cli -- @args
}
