@echo off
setlocal
cd /d "%~dp0"
set "PLAZCODE_APP=PlazCode.exe"
if not exist "%PLAZCODE_APP%" (
  if exist "plazcode-agent.exe" (
    set "PLAZCODE_APP=plazcode-agent.exe"
  ) else (
    echo PlazCode.exe is missing. Check Windows Security Protection history.
    pause
    exit /b 1
  )
)
echo Starting PlazCode...
start "" /D "%~dp0" "%~dp0%PLAZCODE_APP%"
if errorlevel 1 (
  echo Windows could not start PlazCode. Check Protection history for details.
  pause
  exit /b 1
)
