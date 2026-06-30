@echo off
setlocal EnableExtensions
chcp 65001 >nul 2>&1

echo Staging installer to Windows local temp...
set "CLAUDE_ZH_STAGE=%TEMP%\ClaudeDesktopZhCnInstaller"
set "CLAUDE_ZH_SOURCE=%~dp0"
set "CLAUDE_ZH_ORIGINAL_USER_PROFILE=%USERPROFILE%"
set "CLAUDE_ZH_ORIGINAL_APPDATA=%APPDATA%"
set "CLAUDE_ZH_ORIGINAL_LOCALAPPDATA=%LOCALAPPDATA%"
for /f "usebackq delims=" %%S in (`powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value"`) do set "CLAUDE_ZH_ORIGINAL_USER_SID=%%S"

powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "try { $src=$env:CLAUDE_ZH_SOURCE; $dst=$env:CLAUDE_ZH_STAGE; if (Test-Path -LiteralPath $dst) { Remove-Item -LiteralPath $dst -Recurse -Force }; New-Item -ItemType Directory -Path $dst -Force | Out-Null; Copy-Item -LiteralPath (Join-Path $src 'install-windows.bat') -Destination $dst -Force; Copy-Item -LiteralPath (Join-Path $src 'README.md') -Destination $dst -Force -ErrorAction SilentlyContinue; Copy-Item -LiteralPath (Join-Path $src 'scripts') -Destination $dst -Recurse -Force; Copy-Item -LiteralPath (Join-Path $src 'resources') -Destination $dst -Recurse -Force; exit 0 } catch { Write-Host $_.Exception.Message; exit 1 }"
if errorlevel 1 (
    echo.
    echo Failed to copy installer files to local temp.
    echo Please copy the whole claude-desktop-zh-cn folder to a local Windows path, then run install-windows.bat again.
    pause
    exit /b 1
)

echo Requesting administrator privileges...
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "try { function q($v) { $quote=[string][char]34; return $quote + ([string]$v).Replace($quote, ('\' + $quote)) + $quote }; $script=Join-Path $env:CLAUDE_ZH_STAGE 'scripts\install_windows.ps1'; $args=@('-NoProfile','-ExecutionPolicy','Bypass','-File',(q $script),'-Interactive'); if ($env:CLAUDE_ZH_ORIGINAL_USER_SID) { $args += @('-OriginalUserSid',(q $env:CLAUDE_ZH_ORIGINAL_USER_SID)) }; if ($env:CLAUDE_ZH_ORIGINAL_USER_PROFILE) { $args += @('-OriginalUserProfile',(q $env:CLAUDE_ZH_ORIGINAL_USER_PROFILE)) }; if ($env:CLAUDE_ZH_ORIGINAL_APPDATA) { $args += @('-OriginalAppData',(q $env:CLAUDE_ZH_ORIGINAL_APPDATA)) }; if ($env:CLAUDE_ZH_ORIGINAL_LOCALAPPDATA) { $args += @('-OriginalLocalAppData',(q $env:CLAUDE_ZH_ORIGINAL_LOCALAPPDATA)) }; $log=Join-Path $env:CLAUDE_ZH_STAGE 'install-windows-launch.log'; @('script=' + $script, 'args=' + ($args -join ' ')) | Set-Content -LiteralPath $log -Encoding UTF8; Start-Process -FilePath 'powershell.exe' -ArgumentList ($args -join ' ') -WorkingDirectory $env:CLAUDE_ZH_STAGE -Verb RunAs -ErrorAction Stop; exit 0 } catch { Write-Host $_.Exception.Message; exit 1 }"
if errorlevel 1 (
    echo.
    echo Failed to request administrator privileges.
    echo If you cancelled UAC, run this script again.
    pause
    exit /b 1
)

exit /b 0
