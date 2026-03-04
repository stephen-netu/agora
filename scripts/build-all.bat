@echo off
:: build-all.bat — Full release build of Agora (server + CLI + desktop app)
::
:: Produces optimised binaries in target\release\:
::   agora-server.exe   — homeserver
::   agora-cli.exe      — command-line client
::   agora-app.exe      — desktop application
setlocal

for %%i in ("%~dp0..") do set "REPO=%%~fi"
cd /d "%REPO%"

:: ── Frontend ─────────────────────────────────────────────────────────────────
echo =^> Installing frontend dependencies...
npm install --prefix "%REPO%\agora-app\frontend"
if errorlevel 1 ( echo ERROR: npm install failed. & pause & exit /b 1 )

echo =^> Building frontend...
npm run build --prefix "%REPO%\agora-app\frontend"
if errorlevel 1 ( echo ERROR: Frontend build failed. & pause & exit /b 1 )

:: ── Rust workspace ────────────────────────────────────────────────────────────
echo.
echo =^> Building Rust workspace (release)...
cargo build --release -p agora-server -p agora-cli -p agora-app
if errorlevel 1 ( echo ERROR: Rust build failed. & pause & exit /b 1 )

echo.
echo Build complete.
echo.
echo Binaries in %REPO%\target\release\:
echo   agora-server.exe
echo   agora-cli.exe
echo   agora-app.exe
echo.
echo Run scripts\start-server.bat and scripts\start-app.bat to launch.
pause
endlocal
