param(
  [string]$PatcherPath = "",
  [switch]$SelfTest
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

function Join-LocalAppData {
  param([string]$RelativePath)
  Join-Path $env:LOCALAPPDATA $RelativePath
}

function Join-RoamingAppData {
  param([string]$RelativePath)
  Join-Path $env:APPDATA $RelativePath
}

function Resolve-PatcherPath {
  $candidates = @()

  if ($PatcherPath) {
    $candidates += $PatcherPath
  }
  if ($env:CC_DESKTOP_PATCHER) {
    $candidates += $env:CC_DESKTOP_PATCHER
  }

  $candidates += @(
    (Join-Path $RepoRoot "cc_desktop_zh_cn_windows.py"),
    (Join-Path $RepoRoot "..\CC-desktop-zh-cn-WIN-portable\cc_desktop_zh_cn_windows.py"),
    "D:\Develop\claude-desktop\CC-desktop-zh-cn-WIN-portable\cc_desktop_zh_cn_windows.py"
  )

  foreach ($candidate in $candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate)) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }

  return $null
}

function Resolve-PythonCommand {
  $py = Get-Command py -ErrorAction SilentlyContinue
  if ($py) {
    return [pscustomobject]@{
      File = $py.Source
      PrefixArgs = @("-3")
    }
  }

  $python = Get-Command python -ErrorAction SilentlyContinue
  if ($python) {
    return [pscustomobject]@{
      File = $python.Source
      PrefixArgs = @()
    }
  }

  return $null
}

function Get-FileVersionText {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    return ""
  }
  try {
    $info = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($Path)
    if ($info.ProductVersion) {
      return $info.ProductVersion
    }
    if ($info.FileVersion) {
      return $info.FileVersion
    }
  } catch {
    return ""
  }
  return ""
}

function Get-LauncherStatus {
  $exe = Join-LocalAppData "ClaudeZhCN\Claude\Claude.exe"
  $launcher = Join-LocalAppData "ClaudeZhCN\launch_claude_zh_cn.vbs"
  $dataDir = Join-LocalAppData "Claude-3p"
  $configLibrary = Join-Path $dataDir "configLibrary"
  $desktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "Claude zh-CN.lnk"
  $startShortcut = Join-RoamingAppData "Microsoft\Windows\Start Menu\Programs\Claude zh-CN.lnk"
  $patcher = Resolve-PatcherPath
  $python = Resolve-PythonCommand

  $installed = Test-Path -LiteralPath $exe
  $launcherReady = Test-Path -LiteralPath $launcher
  $shortcutReady = (Test-Path -LiteralPath $desktopShortcut) -or (Test-Path -LiteralPath $startShortcut)
  $apiConfigured = Test-Path -LiteralPath $configLibrary
  $version = Get-FileVersionText $exe

  if (-not $installed) {
    $state = "missing"
    $headline = "需要初始化"
    $primary = "开始安装 / 修复"
    $detail = "还没有生成本机汉化版。"
  } elseif (-not $launcherReady -or -not $shortcutReady) {
    $state = "repair"
    $headline = "需要修复"
    $primary = "一键修复"
    $detail = "启动器或快捷方式不完整。"
  } else {
    $state = "ready"
    $headline = "可以直接打开"
    $primary = "打开 Claude zh-CN"
    $detail = "汉化版已安装，快捷方式正常。"
  }

  [pscustomobject]@{
    State = $state
    Headline = $headline
    PrimaryAction = $primary
    Detail = $detail
    Installed = $installed
    Version = $(if ($version) { $version } else { "未检测到" })
    ApiConfigured = $apiConfigured
    LauncherReady = $launcherReady
    ShortcutReady = $shortcutReady
    PatcherPath = $patcher
    PythonPath = $(if ($python) { $python.File } else { "" })
    ExePath = $exe
    LauncherPath = $launcher
    ConfigLibrary = $configLibrary
  }
}

if ($SelfTest) {
  Get-LauncherStatus | ConvertTo-Json -Depth 4
  exit 0
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$script:Status = Get-LauncherStatus
$script:IsBusy = $false
$script:UpdateAvailable = $false

function New-Font {
  param(
    [float]$Size,
    [System.Drawing.FontStyle]$Style = [System.Drawing.FontStyle]::Regular
  )
  New-Object System.Drawing.Font("Microsoft YaHei UI", $Size, $Style)
}

function New-Label {
  param(
    [string]$Text,
    [int]$X,
    [int]$Y,
    [int]$Width,
    [int]$Height,
    [float]$Size = 9,
    [System.Drawing.FontStyle]$Style = [System.Drawing.FontStyle]::Regular,
    [System.Drawing.Color]$Color = [System.Drawing.Color]::FromArgb(23, 35, 38)
  )

  $label = New-Object System.Windows.Forms.Label
  $label.Text = $Text
  $label.Location = New-Object System.Drawing.Point($X, $Y)
  $label.Size = New-Object System.Drawing.Size($Width, $Height)
  $label.Font = New-Font $Size $Style
  $label.ForeColor = $Color
  $label.BackColor = [System.Drawing.Color]::Transparent
  $label
}

function New-Button {
  param(
    [string]$Text,
    [int]$X,
    [int]$Y,
    [int]$Width,
    [int]$Height,
    [bool]$Primary = $false
  )

  $button = New-Object System.Windows.Forms.Button
  $button.Text = $Text
  $button.Location = New-Object System.Drawing.Point($X, $Y)
  $button.Size = New-Object System.Drawing.Size($Width, $Height)
  $button.Font = New-Font 10 ([System.Drawing.FontStyle]::Bold)
  $button.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
  $button.FlatAppearance.BorderSize = 1
  if ($Primary) {
    $button.BackColor = [System.Drawing.Color]::FromArgb(18, 63, 67)
    $button.ForeColor = [System.Drawing.Color]::White
    $button.FlatAppearance.BorderColor = [System.Drawing.Color]::FromArgb(18, 63, 67)
  } else {
    $button.BackColor = [System.Drawing.Color]::White
    $button.ForeColor = [System.Drawing.Color]::FromArgb(23, 35, 38)
    $button.FlatAppearance.BorderColor = [System.Drawing.Color]::FromArgb(214, 222, 219)
  }
  $button
}

function Quote-Arg {
  param([string]$Value)
  '"' + ($Value -replace '"', '\"') + '"'
}

function Append-Log {
  param([string]$Text)
  $timestamp = Get-Date -Format "HH:mm:ss"
  $script:LogBox.AppendText("[$timestamp] $Text`r`n")
  $script:LogBox.SelectionStart = $script:LogBox.TextLength
  $script:LogBox.ScrollToCaret()
}

function Set-Busy {
  param([bool]$Busy)
  $script:IsBusy = $Busy
  $script:Form.Cursor = $(if ($Busy) { [System.Windows.Forms.Cursors]::WaitCursor } else { [System.Windows.Forms.Cursors]::Default })
  foreach ($control in $script:ActionButtons) {
    $control.Enabled = -not $Busy
  }
}

function Invoke-Patcher {
  param(
    [string[]]$PatchArgs,
    [string]$ActionName
  )

  if ($script:IsBusy) {
    return
  }

  $script:Status = Get-LauncherStatus
  if (-not $script:Status.PatcherPath) {
    [System.Windows.Forms.MessageBox]::Show(
      "未找到 cc_desktop_zh_cn_windows.py。可以设置环境变量 CC_DESKTOP_PATCHER，或把脚本放到项目根目录。",
      "缺少核心脚本",
      [System.Windows.Forms.MessageBoxButtons]::OK,
      [System.Windows.Forms.MessageBoxIcon]::Warning
    ) | Out-Null
    Append-Log "未找到核心脚本，无法执行：$ActionName"
    Refresh-Status
    return
  }

  $python = Resolve-PythonCommand
  if (-not $python) {
    [System.Windows.Forms.MessageBox]::Show(
      "未找到 Python 3。请安装 Python 3 或启用 py 启动器。",
      "缺少 Python",
      [System.Windows.Forms.MessageBoxButtons]::OK,
      [System.Windows.Forms.MessageBoxIcon]::Warning
    ) | Out-Null
    Append-Log "未找到 Python，无法执行：$ActionName"
    Refresh-Status
    return
  }

  Set-Busy $true
  Append-Log "开始：$ActionName"

  try {
    $arguments = @()
    $arguments += $python.PrefixArgs
    $arguments += $script:Status.PatcherPath
    $arguments += $PatchArgs
    $argumentText = ($arguments | ForEach-Object { Quote-Arg $_ }) -join " "

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $python.File
    $startInfo.Arguments = $argumentText
    $startInfo.WorkingDirectory = Split-Path -Parent $script:Status.PatcherPath
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true
    $startInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $startInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()

    $exitCode = $process.ExitCode
    if ($ActionName -eq "检查更新") {
      $script:UpdateAvailable = ($exitCode -eq 10)
    } elseif ($exitCode -eq 0 -and ($ActionName -like "更新*" -or $ActionName -like "首次安装*" -or $ActionName -like "修复*")) {
      $script:UpdateAvailable = $false
    }

    if ($stdout.Trim()) {
      Append-Log $stdout.Trim()
    }
    if ($stderr.Trim()) {
      Append-Log $stderr.Trim()
    }
    Append-Log "$ActionName 结束，退出码：$exitCode"
  } catch {
    Append-Log "$ActionName 失败：$($_.Exception.Message)"
  } finally {
    Set-Busy $false
    Refresh-Status
  }
}

function Start-VisibleConsoleAction {
  param(
    [string[]]$PatchArgs,
    [string]$Title
  )

  $script:Status = Get-LauncherStatus
  if (-not $script:Status.PatcherPath) {
    [System.Windows.Forms.MessageBox]::Show(
      "未找到核心脚本，无法打开高级工具。",
      "缺少核心脚本",
      [System.Windows.Forms.MessageBoxButtons]::OK,
      [System.Windows.Forms.MessageBoxIcon]::Warning
    ) | Out-Null
    Refresh-Status
    return
  }

  $python = Resolve-PythonCommand
  if (-not $python) {
    [System.Windows.Forms.MessageBox]::Show(
      "未找到 Python 3。请安装 Python 3 或启用 py 启动器。",
      "缺少 Python",
      [System.Windows.Forms.MessageBoxButtons]::OK,
      [System.Windows.Forms.MessageBoxIcon]::Warning
    ) | Out-Null
    return
  }

  $arguments = @()
  $arguments += $python.PrefixArgs
  $arguments += $script:Status.PatcherPath
  $arguments += $PatchArgs
  $command = Quote-Arg $python.File
  foreach ($argument in $arguments) {
    $command += " " + (Quote-Arg $argument)
  }

  $psArgs = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-NoExit",
    "-Command",
    "& $command"
  )

  Start-Process -FilePath "powershell.exe" -ArgumentList $psArgs -WorkingDirectory (Split-Path -Parent $script:Status.PatcherPath)
  Append-Log "已打开高级工具：$Title"
}

function Invoke-PrimaryAction {
  $script:Status = Get-LauncherStatus
  if (-not $script:Status.Installed) {
    Invoke-Patcher @("--initialize") "首次安装 / 初始化"
    return
  }

  if ($script:UpdateAvailable) {
    Invoke-Patcher @("--force-download") "更新并重新汉化"
    return
  }

  if (-not $script:Status.LauncherReady -or -not $script:Status.ShortcutReady) {
    Invoke-Patcher @("--apply-user-settings", "--create-shortcuts") "修复启动器和快捷方式"
    return
  }

  Invoke-Patcher @("--apply-user-settings") "应用中文设置"
  $script:Status = Get-LauncherStatus

  try {
    if (Test-Path -LiteralPath $script:Status.LauncherPath) {
      Start-Process -FilePath "wscript.exe" -ArgumentList (Quote-Arg $script:Status.LauncherPath)
      Append-Log "已通过兼容启动器打开 Claude zh-CN。"
    } elseif (Test-Path -LiteralPath $script:Status.ExePath) {
      Start-Process -FilePath $script:Status.ExePath -WorkingDirectory (Split-Path -Parent $script:Status.ExePath)
      Append-Log "已直接打开 Claude.exe。建议稍后执行修复以恢复兼容启动器。"
    } else {
      Append-Log "未找到 Claude zh-CN 程序。"
    }
  } catch {
    Append-Log "启动失败：$($_.Exception.Message)"
  }
}

function Refresh-Status {
  $script:Status = Get-LauncherStatus
  $headline = $script:Status.Headline
  $detail = $script:Status.Detail
  $primaryAction = $script:Status.PrimaryAction
  if ($script:UpdateAvailable -and $script:Status.Installed) {
    $headline = "发现新版"
    $detail = "本机版本可更新，原版不会被修改。"
    $primaryAction = "更新并重新汉化"
  }
  $script:HeadlineLabel.Text = $headline
  $script:DetailLabel.Text = $detail
  $script:PrimaryButton.Text = $primaryAction
  $script:InstalledValue.Text = $(if ($script:Status.Installed) { "已安装" } else { "未安装" })
  $script:VersionValue.Text = $script:Status.Version
  $script:ApiValue.Text = $(if ($script:Status.ApiConfigured) { "已配置" } else { "未配置" })
  $script:ShortcutValue.Text = $(if ($script:Status.ShortcutReady) { "正常" } else { "待修复" })
  $script:PatcherValue.Text = $(if ($script:Status.PatcherPath) { "已找到" } else { "未找到" })
  $script:PythonValue.Text = $(if ($script:Status.PythonPath) { "可用" } else { "未找到" })
}

$script:Form = New-Object System.Windows.Forms.Form
$script:Form.Text = "Claude zh-CN 启动器"
$script:Form.StartPosition = "CenterScreen"
$script:Form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::FixedDialog
$script:Form.MaximizeBox = $false
$script:Form.MinimizeBox = $true
$script:Form.ClientSize = New-Object System.Drawing.Size(920, 620)
$script:Form.BackColor = [System.Drawing.Color]::FromArgb(239, 244, 239)
$script:Form.Font = New-Font 9

$brand = New-Label "WIN CC Desktop zh-CN" 28 22 300 22 8 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(85, 101, 105))
$title = New-Label "Claude zh-CN 启动器" 28 43 420 42 22 ([System.Drawing.FontStyle]::Bold)
$script:Form.Controls.AddRange(@($brand, $title))

$heroPanel = New-Object System.Windows.Forms.Panel
$heroPanel.Location = New-Object System.Drawing.Point(28, 95)
$heroPanel.Size = New-Object System.Drawing.Size(550, 155)
$heroPanel.BackColor = [System.Drawing.Color]::FromArgb(231, 242, 241)
$heroPanel.BorderStyle = [System.Windows.Forms.BorderStyle]::FixedSingle
$script:Form.Controls.Add($heroPanel)

$script:HeadlineLabel = New-Label $script:Status.Headline 24 28 350 32 17 ([System.Drawing.FontStyle]::Bold)
$script:DetailLabel = New-Label $script:Status.Detail 24 65 350 26 10 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(64, 81, 85))
$script:PrimaryButton = New-Button $script:Status.PrimaryAction 345 82 178 44 $true
$heroPanel.Controls.AddRange(@($script:HeadlineLabel, $script:DetailLabel, $script:PrimaryButton))

$openButton = New-Button "打开" 28 270 170 42
$updateButton = New-Button "检查更新" 218 270 170 42
$repairButton = New-Button "修复" 408 270 170 42
$script:Form.Controls.AddRange(@($openButton, $updateButton, $repairButton))

$statusPanel = New-Object System.Windows.Forms.Panel
$statusPanel.Location = New-Object System.Drawing.Point(28, 330)
$statusPanel.Size = New-Object System.Drawing.Size(550, 125)
$statusPanel.BackColor = [System.Drawing.Color]::White
$statusPanel.BorderStyle = [System.Windows.Forms.BorderStyle]::FixedSingle
$script:Form.Controls.Add($statusPanel)

$statusPanel.Controls.Add((New-Label "汉化版" 18 16 90 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:InstalledValue = New-Label "" 18 42 110 28 12 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(23, 100, 72))
$statusPanel.Controls.Add($script:InstalledValue)

$statusPanel.Controls.Add((New-Label "本机版本" 150 16 90 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:VersionValue = New-Label "" 150 42 130 28 12 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(23, 100, 72))
$statusPanel.Controls.Add($script:VersionValue)

$statusPanel.Controls.Add((New-Label "API 模式" 295 16 90 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:ApiValue = New-Label "" 295 42 110 28 12 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(23, 100, 72))
$statusPanel.Controls.Add($script:ApiValue)

$statusPanel.Controls.Add((New-Label "快捷方式" 420 16 90 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:ShortcutValue = New-Label "" 420 42 110 28 12 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(23, 100, 72))
$statusPanel.Controls.Add($script:ShortcutValue)

$statusPanel.Controls.Add((New-Label "核心脚本" 18 82 90 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:PatcherValue = New-Label "" 88 82 110 22 9 ([System.Drawing.FontStyle]::Bold)
$statusPanel.Controls.Add($script:PatcherValue)

$statusPanel.Controls.Add((New-Label "Python" 225 82 70 22 9 ([System.Drawing.FontStyle]::Regular) ([System.Drawing.Color]::FromArgb(101, 114, 118))))
$script:PythonValue = New-Label "" 282 82 110 22 9 ([System.Drawing.FontStyle]::Bold)
$statusPanel.Controls.Add($script:PythonValue)

$advancedPanel = New-Object System.Windows.Forms.GroupBox
$advancedPanel.Text = "高级工具"
$advancedPanel.Font = New-Font 10 ([System.Drawing.FontStyle]::Bold)
$advancedPanel.Location = New-Object System.Drawing.Point(28, 472)
$advancedPanel.Size = New-Object System.Drawing.Size(550, 120)
$advancedPanel.BackColor = [System.Drawing.Color]::Transparent
$script:Form.Controls.Add($advancedPanel)

$apiButton = New-Button "API 模式配置" 16 28 160 34
$syncButton = New-Button "导入 / 同步配置" 195 28 160 34
$coworkButton = New-Button "Cowork / VM 修复" 374 28 160 34
$oauthButton = New-Button "OAuth 登录修复" 16 72 160 34
$codeButton = New-Button "Claude Code 管理" 195 72 160 34
$cleanButton = New-Button "清理 / 重置" 374 72 160 34
$cleanButton.ForeColor = [System.Drawing.Color]::FromArgb(161, 59, 49)
$advancedPanel.Controls.AddRange(@($apiButton, $syncButton, $coworkButton, $oauthButton, $codeButton, $cleanButton))

$rightPanel = New-Object System.Windows.Forms.Panel
$rightPanel.Location = New-Object System.Drawing.Point(608, 28)
$rightPanel.Size = New-Object System.Drawing.Size(284, 564)
$rightPanel.BackColor = [System.Drawing.Color]::White
$rightPanel.BorderStyle = [System.Windows.Forms.BorderStyle]::FixedSingle
$script:Form.Controls.Add($rightPanel)

$rightPanel.Controls.Add((New-Label "最近操作" 18 18 150 28 15 ([System.Drawing.FontStyle]::Bold)))
$script:LogBox = New-Object System.Windows.Forms.RichTextBox
$script:LogBox.Location = New-Object System.Drawing.Point(18, 56)
$script:LogBox.Size = New-Object System.Drawing.Size(246, 395)
$script:LogBox.ReadOnly = $true
$script:LogBox.BorderStyle = [System.Windows.Forms.BorderStyle]::None
$script:LogBox.BackColor = [System.Drawing.Color]::FromArgb(248, 250, 247)
$script:LogBox.Font = New-Font 9
$rightPanel.Controls.Add($script:LogBox)

$refreshButton = New-Button "刷新状态" 18 470 116 38
$diagnosticsButton = New-Button "路径 / 诊断" 148 470 116 38
$rightPanel.Controls.AddRange(@($refreshButton, $diagnosticsButton))

$notice = New-Label "危险操作会在执行前再次确认。" 18 524 246 22 9 ([System.Drawing.FontStyle]::Bold) ([System.Drawing.Color]::FromArgb(103, 68, 15))
$rightPanel.Controls.Add($notice)

$script:ActionButtons = @(
  $script:PrimaryButton, $openButton, $updateButton, $repairButton,
  $apiButton, $syncButton, $coworkButton, $oauthButton, $codeButton,
  $cleanButton, $refreshButton, $diagnosticsButton
)

$script:PrimaryButton.Add_Click({ Invoke-PrimaryAction })
$openButton.Add_Click({ Invoke-PrimaryAction })
$updateButton.Add_Click({ Invoke-Patcher @("--check-update") "检查更新" })
$repairButton.Add_Click({ Invoke-Patcher @("--apply-user-settings", "--create-shortcuts") "修复启动器和快捷方式" })
$refreshButton.Add_Click({
  Refresh-Status
  Append-Log "状态已刷新。"
})
$diagnosticsButton.Add_Click({ Invoke-Patcher @("--show-user-data", "--show-oauth-protocol") "路径 / 诊断" })

$apiButton.Add_Click({ Start-VisibleConsoleAction @("--third-party-wizard") "API 模式配置" })
$syncButton.Add_Click({ Start-VisibleConsoleAction @("--import-sync-wizard") "导入 / 同步配置" })
$coworkButton.Add_Click({ Start-VisibleConsoleAction @("--cowork-repair-wizard") "Cowork / VM 修复" })
$oauthButton.Add_Click({
  $result = [System.Windows.Forms.MessageBox]::Show(
    "这会备份当前 claude:// 回调，并临时指向汉化版启动器。通常只在浏览器登录被官方版接走时使用。",
    "OAuth 登录修复确认",
    [System.Windows.Forms.MessageBoxButtons]::OKCancel,
    [System.Windows.Forms.MessageBoxIcon]::Warning
  )
  if ($result -eq [System.Windows.Forms.DialogResult]::OK) {
    Start-VisibleConsoleAction @("--prepare-oauth-login") "OAuth 登录修复"
  } else {
    Append-Log "已取消 OAuth 登录修复。"
  }
})
$codeButton.Add_Click({ Start-VisibleConsoleAction @("--show-claude-code") "Claude Code 管理" })
$cleanButton.Add_Click({
  $result = [System.Windows.Forms.MessageBox]::Show(
    "这会打开清理入口。继续前请确认已经备份重要账号和配置数据。",
    "清理 / 重置确认",
    [System.Windows.Forms.MessageBoxButtons]::OKCancel,
    [System.Windows.Forms.MessageBoxIcon]::Warning
  )
  if ($result -eq [System.Windows.Forms.DialogResult]::OK) {
    Start-VisibleConsoleAction @("--clean-user-data") "清理 / 重置"
  } else {
    Append-Log "已取消清理 / 重置。"
  }
})

Refresh-Status
Append-Log "状态检查完成。"

[void][System.Windows.Forms.Application]::Run($script:Form)
