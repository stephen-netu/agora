@echo off
echo Agora Development Mode
echo ======================
echo.
echo This will:
echo 1. Build the frontend if needed
echo 2. Start the server with cargo watch (auto-rebuild on changes)
echo 3. Start the desktop app
echo.

cd /d "%~dp0"

:: Check if node_modules exists, if not install
if not exist "agora-app\frontend\node_modules" (
    echo Installing frontend dependencies...
    cd agora-app\frontend && npm install && cd \..\..
)

:: Build frontend
echo Building frontend...
cd agora-app\frontend && npm run build && cd \..\..

:: Install cargo-watch if not present
cargo install cargo-watch 2>nul

:: Start server with watch mode in new window
echo Starting server with auto-reload...
start "Agora Server (Dev)" cmd /k "cargo watch -x 'run --bin agora-server'"

:: Wait for server
timeout /t 5 /nobreak >nul

:: Start app
echo Starting desktop app...
start "Agora Desktop" cmd /k "cargo run --bin agora-app"

echo.
echo Development environment started!
echo - Server window: Auto-rebuilds on Rust changes
echo - Desktop window: Manual restart needed for frontend changes
echo - Press Ctrl+C in each window to stop
goto :eof
