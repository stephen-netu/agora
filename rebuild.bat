@echo off
echo Clean rebuild of Agora...
cd /d "%~dp0"

:: Kill any running instances
taskkill /F /IM agora-server.exe 2>nul
taskkill /F /IM agora-app.exe 2>nul
timeout /t 2 /nobreak >nul

:: Clean build
cargo clean
cd agora-app\frontend && npm run build && cd \..\..
cargo build --release --bin agora-server --bin agora-app

echo.
echo Rebuild complete! Run launch-agora-fast.bat to start.
pause
