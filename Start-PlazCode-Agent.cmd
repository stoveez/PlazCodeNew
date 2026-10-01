@echo off
setlocal
cd /d "%~dp0"
if not exist "plazcode-agent.exe" (
  echo plazcode-agent.exe is missing. Check Windows Security Protection history.
  pause
  exit /b 1
)
echo Starting PlazCode Agent...
start "" /D "%~dp0" "%~dp0plazcode-agent.exe"
if errorlevel 1 (
  echo Windows could not start the agent. Check Protection history for details.
  pause
  exit /b 1
)
