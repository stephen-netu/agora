@echo off
:: start-tui.bat — Start the Agora interactive terminal UI
::
:: Usage:
::   start-tui.bat                          connect (TUI)
::   start-tui.bat login -u alice -p pass   any agora-cli subcommand
::   start-tui.bat --help

for %%i in ("%~dp0..") do set "REPO=%%~fi"
cd /d "%REPO%"

if "%~1"=="" (
    cargo run -p agora-cli -- connect
) else (
    cargo run -p agora-cli -- %*
)
