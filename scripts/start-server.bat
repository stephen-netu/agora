@echo off
:: start-server.bat — Start the Agora homeserver
:: Runs on the default address http://127.0.0.1:8008

for %%i in ("%~dp0..") do set "REPO=%%~fi"
cd /d "%REPO%"

echo Starting Agora server...
echo Address: http://127.0.0.1:8008
echo Press Ctrl+C to stop.
echo.

cargo run -p agora-server
