@echo off
:: start-app.bat — Start the Agora desktop application (Tauri)
::
:: Builds the frontend if it hasn't been built yet, then launches the app.
:: For a full rebuild of the frontend use: build-all.bat
setlocal

for %%i in ("%~dp0..") do set "REPO=%%~fi"
cd /d "%REPO%"

:: Build the frontend if the build directory doesn't exist
if not exist "%REPO%\agora-app\frontend\build\" (
    echo Frontend build not found -- building now...

    if not exist "%REPO%\agora-app\frontend\node_modules\" (
        echo Installing frontend dependencies...
        npm install --prefix "%REPO%\agora-app\frontend"
        if errorlevel 1 (
            echo ERROR: npm install failed.
            pause
            exit /b 1
        )
    )

    npm run build --prefix "%REPO%\agora-app\frontend"
    if errorlevel 1 (
        echo ERROR: Frontend build failed.
        pause
        exit /b 1
    )
    echo.
)

echo Starting Agora desktop app...
echo Press Ctrl+C to stop.
echo.

cargo run -p agora-app
endlocal
