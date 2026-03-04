# build-all.ps1 — Full release build of Agora (server + CLI + desktop app)
#
# Produces optimised binaries in target\release\:
#   agora-server.exe   — homeserver
#   agora-cli.exe      — command-line client
#   agora-app.exe      — desktop application
#
# If execution policy blocks this, run once:
#   Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
$ErrorActionPreference = 'Stop'

$RepoRoot    = Split-Path -Parent $PSScriptRoot
$FrontendDir = Join-Path $RepoRoot 'agora-app' 'frontend'

Set-Location $RepoRoot

# ── Frontend ──────────────────────────────────────────────────────────────────
Write-Host '==> Installing frontend dependencies...'
npm install --prefix $FrontendDir

Write-Host '==> Building frontend...'
npm run build --prefix $FrontendDir

# ── Rust workspace ────────────────────────────────────────────────────────────
Write-Host ''
Write-Host '==> Building Rust workspace (release)...'
& cargo build --release -p agora-server -p agora-cli -p agora-app

$BinDir = Join-Path $RepoRoot 'target' 'release'

Write-Host ''
Write-Host 'Build complete.'
Write-Host ''
Write-Host 'Binaries:'
Write-Host "  $BinDir\agora-server.exe"
Write-Host "  $BinDir\agora-cli.exe"
Write-Host "  $BinDir\agora-app.exe"
Write-Host ''
Write-Host 'Run scripts\start-server.ps1 and scripts\start-app.ps1 to launch.'
