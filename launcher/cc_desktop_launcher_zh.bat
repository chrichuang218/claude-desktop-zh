@echo off
chcp 65001 > nul
setlocal

powershell.exe -NoProfile -ExecutionPolicy Bypass -STA -File "%~dp0cc_desktop_launcher_zh.ps1" %*
set "ERR=%ERRORLEVEL%"

if not "%ERR%"=="0" (
  echo.
  echo Claude 中文启动器已退出，退出码：%ERR%
  pause
)

exit /b %ERR%
