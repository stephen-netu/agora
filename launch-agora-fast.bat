@echo off
echo Starting Agora (Fast Mode)...
cd /d "D:\Projects\Agora\agora"

:: Check if server is already running
tasklist /FI "IMAGENAME eq agora-server.exe" 2>nul | find /I "agora-server.exe" >nul
if %ERRORLEVEL% EQU 0 (
    echo.
    echo WARNING: Agora server is already running!
    echo.
    echo Options:
    echo [1] Kill existing and restart
    echo [2] Cancel
    echo.
    set /p choice="Enter choice (1-2): "
    if "!choice!"=="1" (
        echo Stopping existing server...
        taskkill /F /IM agora-server.exe 2>nul
        taskkill /F /IM agora-app.exe 2>nul
        timeout /t 2 /nobreak >nul
    ) else (
        echo Cancelled.
        pause
        exit /b
    )
)

:: Start the server in a new window
start "Agora Server" "D:\Projects\Agora\agora\target\debug\agora-server.exe"

:: Wait for server to initialize
timeout /t 2 /nobreak >nul

:: Start the desktop app
start "Agora Desktop" "D:\Projects\Agora\agora\target\debug\agora-app.exe"

echo Agora launched!
echo - Server: http://127.0.0.1:8008
echo - Close the command windows to stop
pause
