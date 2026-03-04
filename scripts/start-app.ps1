# start-app.ps1 — Start the Agora desktop application (Tauri)
#
# Builds the frontend if it hasn't been built yet, then launches the app.
# For a full rebuild of the frontend use: build-all.ps1
#
# If execution policy blocks this, run once:
#   Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
$ErrorActionPreference = 'Stop'

$RepoRoot    = Split-Path -Parent $PSScriptRoot
$FrontendDir = Join-Path $RepoRoot 'agora-app' 'frontend'
$FrontendBuild = Join-Path $FrontendDir 'build'

Set-Location $RepoRoot

# Build the frontend if the build directory doesn't exist
if (-not (Test-Path $FrontendBuild)) {
    Write-Host 'Frontend build not found -- building now...'

    if (-not (Test-Path (Join-Path $FrontendDir 'node_modules'))) {
        Write-Host 'Installing frontend dependencies...'
        npm install --prefix $FrontendDir
    }

    npm run build --prefix $FrontendDir
    Write-Host ''
}

Write-Host 'Starting Agora desktop app...'
Write-Host 'Press Ctrl+C to stop.'
Write-Host ''

& cargo run -p agora-app
