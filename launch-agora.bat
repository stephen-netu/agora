@echo off
echo Starting Agora...
cd /d "%~dp0"

:: Start the server in a new window
start "Agora Server" cmd /c "cargo run --bin agora-server"

:: Wait a moment for server to initialize
timeout /t 3 /nobreak >nul

:: Start the desktop app
start "Agora Desktop" cmd /c "cargo run --bin agora-app"

echo Agora launched! Server and Desktop app are starting...
pause
