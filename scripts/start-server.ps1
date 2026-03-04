# start-server.ps1 — Start the Agora homeserver
# Runs on the default address http://127.0.0.1:8008
#
# If execution policy blocks this, run once:
#   Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

Write-Host 'Starting Agora server...'
Write-Host 'Address: http://127.0.0.1:8008'
Write-Host 'Press Ctrl+C to stop.'
Write-Host ''

& cargo run -p agora-server
