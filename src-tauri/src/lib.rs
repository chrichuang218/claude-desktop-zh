use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Serialize)]
struct LauncherStatus {
    state: String,
    installed: bool,
    localized: bool,
    version: String,
    launcher_ready: bool,
    shortcut_ready: bool,
    patcher_ready: bool,
    python_ready: bool,
    engine_ready: bool,
    backup_ready: bool,
    language: String,
    install_path: String,
    engine_path: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ActionResult {
    ok: bool,
    state: String,
    message: String,
    log: String,
}

#[derive(Debug, Serialize)]
struct LiveLog {
    log: String,
    path: String,
}

#[derive(Debug)]
struct WingetMetadata {
    version: String,
    installer_url: Option<String>,
    sha256: Option<String>,
}

const PATCH_ENGINE_ZIP_URL: &str =
    "https://github.com/javaht/claude-desktop-zh-cn/archive/refs/heads/main.zip";
const ELEVATED_HELPER_ARG: &str = "--run-patch-engine-elevated-helper";
const ELEVATED_CHECK_UPDATE_ARG: &str = "--run-check-update-elevated-helper";
const PRODUCT_DATA_DIR: &str = "ClaudeDesktopCN";
const PATCH_ENGINE_DIR: &str = "patch-engine";

#[tauri::command]
fn get_status() -> LauncherStatus {
    read_status()
}

#[tauri::command]
fn open_claude() -> ActionResult {
    let status = read_status();
    if !status.installed {
        return ActionResult::error("missing", claude_install_missing_message());
    }
    if !status.localized {
        return ActionResult::error("repair", "请先安装中文补丁。");
    }

    let Some(exe) = claude_exe_path() else {
        return ActionResult::error("missing", claude_install_missing_message());
    };
    match launch_claude_from_path(&exe) {
        Ok(_) => ActionResult {
            ok: true,
            state: "ready".to_string(),
            message: "已打开 Claude Desktop。".to_string(),
            log: String::new(),
        },
        Err(error) => ActionResult::error("ready", &format!("启动失败：{error}")),
    }
}

#[tauri::command]
fn get_live_log() -> LiveLog {
    let engine = patch_engine_path();
    let stdout_log = engine.join("run-from-claude-desktop-ui.stdout.log");
    let stderr_log = engine.join("run-from-claude-desktop-ui.stderr.log");
    let log = combined_patch_engine_log(&engine, &stdout_log, &stderr_log);

    LiveLog {
        log: tail_text(&log, 90_000),
        path: engine.display().to_string(),
    }
}

#[tauri::command]
async fn check_update() -> ActionResult {
    run_blocking_action(check_update_elevated).await
}

fn check_update_elevated() -> ActionResult {
    if is_current_process_elevated() {
        return check_update_inner();
    }

    match run_self_elevated_check_update() {
        Ok(result) => result,
        Err(error) => ActionResult::error("repair", &format!("管理员检查更新失败：{error}")),
    }
}

fn check_update_inner() -> ActionResult {
    let status = read_status();
    if !status.installed {
        return ActionResult::error("missing", "尚未生成 Claude zh-CN，暂时无法检查更新。");
    }

    let metadata = match query_winget_metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            return ActionResult {
                ok: false,
                state: status.state,
                message: "暂时无法从 winget 读取最新版本。".to_string(),
                log: error,
            };
        }
    };

    let current_version = status.version.clone();
    let comparison = compare_versions(&metadata.version, &current_version);
    let log = update_log(&current_version, &metadata);

    if comparison == Ordering::Greater {
        return ActionResult {
            ok: true,
            state: "update".to_string(),
            message: format!(
                "发现 Claude 新版本 {}。",
                display_version(&metadata.version)
            ),
            log,
        };
    }

    ActionResult {
        ok: true,
        state: status.state,
        message: format!("当前已经是最新版本 {}。", display_version(&current_version)),
        log,
    }
}

#[tauri::command]
async fn repair() -> ActionResult {
    run_blocking_action(|| run_patch_engine_action("install", "zh-CN", "safe")).await
}

#[tauri::command]
async fn install_patch(language: String, patch_mode: String) -> ActionResult {
    let Ok(language) = normalize_language(&language) else {
        return ActionResult::error("repair", "不支持的语言。");
    };
    let Ok(patch_mode) = normalize_patch_mode(&patch_mode) else {
        return ActionResult::error("repair", "不支持的安装模式。");
    };
    run_blocking_action(move || run_patch_engine_action("install", language, patch_mode)).await
}

#[tauri::command]
async fn restore_patch() -> ActionResult {
    run_blocking_action(|| run_patch_engine_action("uninstall", "zh-CN", "safe")).await
}

#[tauri::command]
async fn set_auto_updates(enabled: bool) -> ActionResult {
    let action = if enabled {
        "enable-updates"
    } else {
        "disable-updates"
    };
    run_blocking_action(move || run_patch_engine_action(action, "zh-CN", "safe")).await
}

impl ActionResult {
    fn error(state: &str, message: &str) -> Self {
        Self {
            ok: false,
            state: state.to_string(),
            message: message.to_string(),
            log: message.to_string(),
        }
    }
}

async fn run_blocking_action<F>(action: F) -> ActionResult
where
    F: FnOnce() -> ActionResult + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(action).await {
        Ok(result) => result,
        Err(error) => ActionResult::error("repair", &format!("后台任务失败：{error}")),
    }
}

fn read_status() -> LauncherStatus {
    let install_path = claude_exe_path();
    let installed = install_path.is_some();
    let resources_path = claude_resources_path();
    let localized = resources_path
        .as_ref()
        .map(|path| has_zh_resources(path))
        .unwrap_or(false);
    let backup_ready = resources_path
        .as_ref()
        .map(|path| backup_root(path).exists())
        .unwrap_or(false);
    let launcher_ready = installed;
    let shortcut_ready = installed;
    let engine_path = patch_engine_path();
    let engine_ready = true;
    let patcher_ready = engine_ready;
    let python_ready = command_exists("powershell");
    let version = read_file_version().unwrap_or_else(|| "未检测到".to_string());
    let language = read_claude_locale().unwrap_or_else(|| {
        if localized {
            "zh-CN".to_string()
        } else {
            "未设置".to_string()
        }
    });

    let (state, message) = if !installed {
        ("missing", claude_install_missing_message())
    } else if !localized {
        ("repair", "Claude Desktop 已安装，尚未应用中文补丁。")
    } else if !python_ready {
        ("repair", "未找到 PowerShell，无法运行 Windows 汉化脚本。")
    } else {
        ("ready", "Claude Desktop 中文版可以打开。")
    };

    LauncherStatus {
        state: state.to_string(),
        installed,
        localized,
        version,
        launcher_ready,
        shortcut_ready,
        patcher_ready,
        python_ready,
        engine_ready,
        backup_ready,
        language,
        install_path: install_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        engine_path: engine_path.display().to_string(),
        message: message.to_string(),
    }
}

fn normalize_language(value: &str) -> Result<&'static str, String> {
    match value {
        "zh-CN" | "简体中文" => Ok("zh-CN"),
        "zh-TW" | "繁体中文（中国台湾）" => Ok("zh-TW"),
        "zh-HK" | "繁体中文（中国香港）" => Ok("zh-HK"),
        _ => Err(value.to_string()),
    }
}

fn normalize_patch_mode(value: &str) -> Result<&'static str, String> {
    match value {
        "safe" | "compat" | "兼容模式" => Ok("safe"),
        "official" | "full" | "完整模式" => Ok("official"),
        _ => Err(value.to_string()),
    }
}

fn run_patch_engine_action(action: &str, language: &str, patch_mode: &str) -> ActionResult {
    let status = read_status();
    if !status.installed {
        return ActionResult::error("missing", claude_install_missing_message());
    }
    if !command_exists("powershell") {
        return ActionResult::error("repair", "未找到 PowerShell，无法运行 Windows 汉化脚本。");
    }

    let engine = match ensure_patch_engine() {
        Ok(path) => path,
        Err(error) => return ActionResult::error("repair", &error),
    };

    match run_patch_engine_elevated(&engine, action, language, patch_mode) {
        Ok(log) => {
            let next_status = read_status();
            ActionResult {
                ok: true,
                state: next_status.state,
                message: patch_engine_success_message(action),
                log,
            }
        }
        Err(error) => ActionResult {
            ok: false,
            state: "repair".to_string(),
            message: patch_engine_failure_message(action),
            log: error,
        },
    }
}

fn patch_engine_success_message(action: &str) -> String {
    match action {
        "install" => "中文补丁已安装。".to_string(),
        "uninstall" => "已恢复原样。".to_string(),
        "disable-updates" => "已禁止 Claude Desktop 自动更新。".to_string(),
        "enable-updates" => "已允许 Claude Desktop 自动更新。".to_string(),
        _ => "操作完成。".to_string(),
    }
}

fn patch_engine_failure_message(action: &str) -> String {
    match action {
        "install" => "安装中文补丁失败。".to_string(),
        "uninstall" => "恢复原样失败。".to_string(),
        "disable-updates" | "enable-updates" => "更新设置失败。".to_string(),
        _ => "操作失败。".to_string(),
    }
}

fn ensure_patch_engine() -> Result<PathBuf, String> {
    let engine = patch_engine_path();
    let parent = engine
        .parent()
        .ok_or_else(|| "无法确定补丁引擎目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建补丁引擎目录失败：{error}"))?;

    let temp = parent.join("patch-engine-download");
    let zip = parent.join("patch-engine-main.zip");
    let stage = parent.join("patch-engine-next");
    let command = format!(
        "$ErrorActionPreference='Stop';\
         $dst={dst}; $tmp={tmp}; $zip={zip}; $stage={stage};\
         if (Test-Path -LiteralPath $tmp) {{ Remove-Item -LiteralPath $tmp -Recurse -Force }};\
         if (Test-Path -LiteralPath $zip) {{ Remove-Item -LiteralPath $zip -Force }};\
         if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }};\
         New-Item -ItemType Directory -Path $tmp -Force | Out-Null;\
         Invoke-WebRequest -Uri {url} -OutFile $zip -UseBasicParsing;\
         Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force;\
         $inner = Get-ChildItem -LiteralPath $tmp -Directory | Select-Object -First 1;\
         if (-not $inner) {{ throw 'GitHub 压缩包内容为空。' }};\
         $script = Join-Path $inner.FullName 'scripts\\install_windows.ps1';\
         if (-not (Test-Path -LiteralPath $script)) {{ throw 'GitHub 压缩包缺少 Windows 安装脚本。' }};\
         Move-Item -LiteralPath $inner.FullName -Destination $stage;\
         if (Test-Path -LiteralPath $dst) {{ Remove-Item -LiteralPath $dst -Recurse -Force }};\
         Move-Item -LiteralPath $stage -Destination $dst;\
         Remove-Item -LiteralPath $tmp -Recurse -Force;\
         Remove-Item -LiteralPath $zip -Force",
        dst = ps_path(&engine),
        tmp = ps_path(&temp),
        zip = ps_path(&zip),
        stage = ps_path(&stage),
        url = ps_string(PATCH_ENGINE_ZIP_URL),
    );

    let mut powershell = Command::new("powershell.exe");
    hide_console_window(&mut powershell);
    let output = powershell
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &command,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("下载补丁引擎失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "下载补丁引擎失败：{}",
            process_output_text(&output.stdout, &output.stderr)
        ));
    }

    if patch_engine_script_path(&engine).exists() {
        Ok(engine)
    } else {
        Err("补丁引擎下载完成，但缺少 Windows 安装脚本。".to_string())
    }
}

fn run_patch_engine_elevated(
    engine: &Path,
    action: &str,
    language: &str,
    patch_mode: &str,
) -> Result<String, String> {
    let script = prepare_patch_engine_appx_script(engine)?;
    let launcher = engine.join("run-from-claude-desktop-ui.ps1");
    let completion = engine.join("run-from-claude-desktop-ui.exitcode");
    let stdout_log = engine.join("run-from-claude-desktop-ui.stdout.log");
    let stderr_log = engine.join("run-from-claude-desktop-ui.stderr.log");
    let script_log = engine.join("install-windows.log");
    let _ = fs::remove_file(&completion);
    let _ = fs::remove_file(&stdout_log);
    let _ = fs::remove_file(&stderr_log);
    let _ = fs::remove_file(&script_log);
    let original_sid = current_user_sid().unwrap_or_default();
    let original_profile = env::var("USERPROFILE").unwrap_or_default();
    let original_appdata = env::var("APPDATA").unwrap_or_default();
    let original_local_appdata = env::var("LOCALAPPDATA").unwrap_or_default();

    let launcher_content = patch_engine_launcher_content(
        engine,
        &script,
        &completion,
        action,
        language,
        patch_mode,
        &original_sid,
        &original_profile,
        &original_appdata,
        &original_local_appdata,
    );

    write_powershell_script(&launcher, &launcher_content)
        .map_err(|error| format!("创建补丁启动脚本失败：{error}"))?;

    let mut launch_output = String::new();
    if is_current_process_elevated() {
        spawn_elevated_helper(&launcher, &completion, &stdout_log, &stderr_log, engine)
            .map_err(|error| format!("启动管理员补丁进程失败：{error}"))?;
    } else {
        let exe = env::current_exe().map_err(|error| format!("定位当前程序失败：{error}"))?;
        let argument_list = helper_argument_list(&launcher, &completion, &stdout_log, &stderr_log);
        let command = elevated_start_command(&exe, &argument_list, engine);

        let mut powershell = Command::new("powershell.exe");
        hide_console_window(&mut powershell);
        let output = powershell
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &command,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("启动管理员补丁进程失败：{error}"))?;

        if !output.status.success() {
            return Err(process_output_text(&output.stdout, &output.stderr));
        }
        launch_output = process_output_text(&output.stdout, &output.stderr);
    }

    let exit_code = wait_for_elevated_completion(&completion, Duration::from_secs(20 * 60))?;
    let log = combined_patch_engine_log(engine, &stdout_log, &stderr_log);
    if exit_code == 0 {
        return Ok(if log.is_empty() {
            "管理员补丁进程已完成。".to_string()
        } else {
            log
        });
    }

    if log.is_empty() {
        Err(launch_output)
    } else {
        Err(format!("{launch_output}\n\n{log}"))
    }
}

fn patch_engine_launcher_content(
    engine: &Path,
    script: &Path,
    completion: &Path,
    action: &str,
    language: &str,
    patch_mode: &str,
    original_sid: &str,
    original_profile: &str,
    original_appdata: &str,
    original_local_appdata: &str,
) -> String {
    format!(
        "$ErrorActionPreference='Stop'\n\
         $exitCode = 1\n\
         $exitCodePath = {completion}\n\
         try {{\n\
         Set-Location -LiteralPath {engine}\n\
         $originalSid = {sid}\n\
         $packageRoot = Get-ChildItem -LiteralPath 'C:\\Program Files\\WindowsApps' -Directory -Filter 'Claude_*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1\n\
         if ($packageRoot) {{\n\
             $appPath = Join-Path $packageRoot.FullName 'app'\n\
             if ((Test-Path -LiteralPath $appPath) -and $originalSid) {{\n\
                 & takeown.exe /F $appPath /A /R /D Y | Out-Null\n\
                 $grant = '*' + $originalSid + ':(OI)(CI)M'\n\
                 & icacls.exe $appPath /grant $grant /T /C /Q | Out-Null\n\
             }}\n\
         }}\n\
         & {script} {action} {language} -PatchMode {patch_mode} \
         -OriginalUserSid {sid} \
         -OriginalUserProfile {profile} \
         -OriginalAppData {appdata} \
         -OriginalLocalAppData {local_appdata}\n\
         $exitCode = $LASTEXITCODE\n\
         }} catch {{\n\
             Write-Host $_.Exception.Message\n\
             $exitCode = 1\n\
         }} finally {{\n\
             [System.IO.File]::WriteAllText($exitCodePath, [string]$exitCode, [System.Text.Encoding]::ASCII)\n\
         }}\n\
         exit $exitCode\n",
        completion = ps_path(&completion),
        engine = ps_path(engine),
        script = ps_path(&script),
        action = ps_string(action),
        language = ps_string(language),
        patch_mode = ps_string(patch_mode),
        sid = ps_string(&original_sid),
        profile = ps_string(&original_profile),
        appdata = ps_string(&original_appdata),
        local_appdata = ps_string(&original_local_appdata),
    )
}

fn spawn_elevated_helper(
    launcher: &Path,
    completion: &Path,
    stdout_log: &Path,
    stderr_log: &Path,
    working_dir: &Path,
) -> std::io::Result<()> {
    let exe = env::current_exe()?;
    let mut command = Command::new(exe);
    hide_console_window(&mut command);
    command
        .arg(ELEVATED_HELPER_ARG)
        .arg(launcher)
        .arg(completion)
        .arg(stdout_log)
        .arg(stderr_log)
        .current_dir(working_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn helper_argument_list(
    launcher: &Path,
    completion: &Path,
    stdout_log: &Path,
    stderr_log: &Path,
) -> String {
    format!(
        "{} \"{}\" \"{}\" \"{}\" \"{}\"",
        ELEVATED_HELPER_ARG,
        launcher.display(),
        completion.display(),
        stdout_log.display(),
        stderr_log.display()
    )
}

fn elevated_start_command(file_path: &Path, argument_list: &str, working_dir: &Path) -> String {
    format!(
        "Start-Process -FilePath {file} -ArgumentList {args} -WorkingDirectory {cwd} -Verb RunAs -WindowStyle Hidden | Out-Null",
        file = ps_path(file_path),
        args = ps_string(argument_list),
        cwd = ps_path(working_dir),
    )
}

fn wait_for_elevated_completion(completion: &Path, timeout: Duration) -> Result<i32, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if completion.exists() {
            let raw = read_text_if_present(completion);
            let trimmed = raw.trim_start_matches('\u{feff}').trim();
            return trimmed
                .parse::<i32>()
                .map_err(|_| format!("管理员补丁进程返回了无效退出码：{trimmed}"));
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err("管理员补丁进程超时，请检查是否有未处理的 UAC 确认窗口。".to_string())
}

fn run_self_elevated_check_update() -> Result<ActionResult, String> {
    let work_dir = local_app_data().join(PRODUCT_DATA_DIR);
    fs::create_dir_all(&work_dir).map_err(|error| format!("创建工作目录失败：{error}"))?;
    let result_path = work_dir.join("check-update-result.json");
    let _ = fs::remove_file(&result_path);

    let exe = env::current_exe().map_err(|error| format!("定位当前程序失败：{error}"))?;
    let argument_list = check_update_helper_argument_list(&result_path);
    let command = elevated_start_command(&exe, &argument_list, &work_dir);

    let mut powershell = Command::new("powershell.exe");
    hide_console_window(&mut powershell);
    let output = powershell
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &command,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("启动管理员检查更新进程失败：{error}"))?;

    if !output.status.success() {
        return Err(process_output_text(&output.stdout, &output.stderr));
    }

    wait_for_elevated_action_result(&result_path, Duration::from_secs(5 * 60))
}

fn wait_for_elevated_action_result(path: &Path, timeout: Duration) -> Result<ActionResult, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            let raw = read_text_if_present(path);
            return serde_json::from_str(&raw)
                .map_err(|error| format!("管理员检查更新结果解析失败：{error}"));
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err("管理员检查更新进程超时，请检查是否有未处理的 UAC 确认窗口。".to_string())
}

fn check_update_helper_argument_list(result_path: &Path) -> String {
    format!(
        "{} \"{}\"",
        ELEVATED_CHECK_UPDATE_ARG,
        result_path.display()
    )
}

fn prepare_patch_engine_appx_script(engine: &Path) -> Result<PathBuf, String> {
    let source_path = patch_engine_script_path(engine);
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("读取 Windows 汉化脚本失败：{error}"))?;
    let marker = "$script:DetectedUnpackagedClaudePaths = @(Get-UnpackagedClaudePaths)";
    let replacement = "$script:DetectedUnpackagedClaudePaths = @()";
    if !source.contains(marker) {
        return Err("Windows 汉化脚本结构已变化，无法强制选择 WindowsApps 目录。".to_string());
    }

    let patched = source
        .replace(marker, replacement)
        .replace(
            "Enable-WriteAccess $resourcesPath",
            "Write-Host \"  跳过脚本内部权限更新；由启动器提前处理 WindowsApps ACL。\" -ForegroundColor DarkGray",
        );
    let patched = replace_powershell_function(
        &patched,
        "Patch-HardcodedFrontendStrings",
        FAST_HARDCODED_FRONTEND_PATCH_FUNCTION,
    )?;
    let patched = replace_powershell_function(
        &patched,
        "Restart-Claude",
        WINDOWSAPPS_SAFE_RESTART_FUNCTION,
    )?;
    let patched =
        replace_powershell_function(&patched, "Find-ClaudePath", WINDOWSAPPS_FIND_CLAUDE_PATH)?;
    let patched = replace_powershell_function(
        &patched,
        "Get-ClaudeConfigPaths",
        WINDOWSAPPS_GET_CLAUDE_CONFIG_PATHS,
    )?;
    let patched = replace_powershell_function(
        &patched,
        "Uninstall-WindowsLanguagePack",
        WINDOWSAPPS_SAFE_UNINSTALL_FUNCTION,
    )?;
    let patched_path = engine
        .join("scripts")
        .join("install_windows_force_windowsapps.ps1");
    write_powershell_script(&patched_path, &patched)
        .map_err(|error| format!("写入 WindowsApps 专用脚本失败：{error}"))?;
    Ok(patched_path)
}

fn replace_powershell_function(
    source: &str,
    function_name: &str,
    replacement: &str,
) -> Result<String, String> {
    let marker = format!("function {function_name}");
    let start = source
        .find(&marker)
        .ok_or_else(|| format!("Windows 汉化脚本缺少 {function_name} 函数。"))?;
    let body_start = source[start..]
        .find('{')
        .map(|index| start + index)
        .ok_or_else(|| format!("Windows 汉化脚本 {function_name} 函数结构异常。"))?;

    let mut depth = 0i32;
    for (index, ch) in source[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = body_start + index + ch.len_utf8();
                    let mut next =
                        String::with_capacity(source.len() - (end - start) + replacement.len() + 2);
                    next.push_str(&source[..start]);
                    next.push_str(replacement.trim_end());
                    next.push_str("\n");
                    next.push_str(&source[end..]);
                    return Ok(next);
                }
            }
            _ => {}
        }
    }

    Err(format!(
        "Windows 汉化脚本 {function_name} 函数缺少结束括号。"
    ))
}

const FAST_HARDCODED_FRONTEND_PATCH_FUNCTION: &str = r#"
function Patch-HardcodedFrontendStrings {
    param(
        [string]$ResourcesPath,
        [string]$Language
    )

    $assetsDir = Join-Path $ResourcesPath "ion-dist\assets\v1"
    $jsFiles = @(Get-ChildItem (Join-Path $assetsDir "*.js") -ErrorAction SilentlyContinue)
    if ($jsFiles.Count -eq 0) {
        throw "未找到前端 JS bundle: $assetsDir"
    }

    $plainMap = @{}
    $plainSources = New-Object System.Collections.Generic.List[string]
    $rawPairs = New-Object System.Collections.Generic.List[object]
    foreach ($pair in @(Get-FrontendHardcodedReplacements $Language)) {
        $source = [string]$pair[0]
        $target = [string]$pair[1]
        if (Test-StructuralJsReplacement $source) {
            continue
        }
        if (Test-PlainUiTextReplacement $source) {
            if (-not $plainMap.ContainsKey($source)) {
                $plainMap[$source] = $target
                [void]$plainSources.Add($source)
            }
        } else {
            [void]$rawPairs.Add(@($source, $target))
        }
    }

    $plainRegex = $null
    if ($plainSources.Count -gt 0) {
        $escaped = foreach ($source in $plainSources) {
            [System.Text.RegularExpressions.Regex]::Escape($source)
        }
        $quoteClass = '["' + "'" + [char]96 + ']'
        $pattern = '(?<quote>' + $quoteClass + ')(?<source>' + ($escaped -join '|') + ')\k<quote>'
        $plainRegex = [System.Text.RegularExpressions.Regex]::new(
            $pattern,
            [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
        )
    }

    $patchedFiles = 0
    $patchedStrings = 0
    $fileIndex = 0
    foreach ($file in $jsFiles) {
        $fileIndex += 1
        if (($fileIndex -eq 1) -or ($fileIndex % 50 -eq 0) -or ($fileIndex -eq $jsFiles.Count)) {
            Write-Host "  scanning frontend bundles: $fileIndex/$($jsFiles.Count)" -ForegroundColor DarkGray
        }

        $text = [System.IO.File]::ReadAllText($file.FullName, [System.Text.Encoding]::UTF8)
        $patched = $text
        $count = 0

        foreach ($pair in $rawPairs) {
            $source = [string]$pair[0]
            if (-not $patched.Contains($source)) {
                continue
            }
            $target = [string]$pair[1]
            $index = $patched.IndexOf($source, [System.StringComparison]::Ordinal)
            while ($index -ge 0) {
                $count += 1
                $index = $patched.IndexOf($source, $index + $source.Length, [System.StringComparison]::Ordinal)
            }
            $patched = $patched.Replace($source, $target)
        }

        if ($patched.Contains('"low","medium","high","xhigh","max"')) {
            $script:__effortLabelReplacementCount = 0
            $effortLabelPattern = '(?<prefix>label:)(?<item>[$A-Za-z_][$\w]*)\.name,value:\k<item>\.id,checked:(?<checked>[^,{}]+),onSelect:\(\)=>(?<select>[$A-Za-z_][$\w]*)\(\k<item>\.id,!1\)\}'
            $effortLabelRegex = [System.Text.RegularExpressions.Regex]::new(
                $effortLabelPattern,
                [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
            )
            $patched = $effortLabelRegex.Replace($patched, {
                param($match)
                $script:__effortLabelReplacementCount += 1
                $item = $match.Groups["item"].Value
                $checked = $match.Groups["checked"].Value
                $select = $match.Groups["select"].Value
                return 'label:({low:"低",medium:"中",high:"高",xhigh:"超高",max:"最高"}[' + $item + '.id]??' + $item + '.name),value:' + $item + '.id,checked:' + $checked + ',onSelect:()=>'+ $select + '(' + $item + '.id,!1)}'
            })
            $count += $script:__effortLabelReplacementCount
            $script:__effortLabelReplacementCount = 0
        }

        if ($plainRegex) {
            $script:__frontendReplacementCount = 0
            $patched = $plainRegex.Replace($patched, {
                param($match)
                $source = $match.Groups["source"].Value
                $target = $plainMap[$source]
                if ($null -eq $target) {
                    return $match.Value
                }
                $script:__frontendReplacementCount += 1
                $quote = $match.Groups["quote"].Value
                return $quote + $target + $quote
            })
            $count += $script:__frontendReplacementCount
            $script:__frontendReplacementCount = 0
        }

        if ($patched -ne $text) {
            Backup-ModifiedFile $ResourcesPath $file.FullName
            [System.IO.File]::WriteAllText($file.FullName, $patched, $Utf8NoBom)
            $patchedFiles += 1
            $patchedStrings += $count
        }
    }

    Write-Host "  patched hardcoded frontend strings: $patchedStrings replacements in $patchedFiles files" -ForegroundColor Green
}
"#;

const WINDOWSAPPS_SAFE_RESTART_FUNCTION: &str = r#"
function Restart-Claude {
    param([string]$ClaudePath)

    Stop-ClaudeProcesses

    $exe = Get-ClaudeExePath $ClaudePath
    if (-not $exe) {
        Write-Host "  [警告] 未找到 Claude.exe，请手动启动 Claude Desktop。" -ForegroundColor DarkYellow
        return
    }

    try {
        $resolvedExe = Resolve-Path -LiteralPath $exe -ErrorAction Stop
        $packageRoot = $resolvedExe.Path -split "\\app\\", 2 | Select-Object -First 1
        $packageName = Split-Path -Leaf $packageRoot
        if ($packageName -match "^([^_]+)_.+__([^_]+)$") {
            $appId = "$($Matches[1])_$($Matches[2])!$($Matches[1])"
            Start-Process explorer.exe "shell:AppsFolder\$appId" -ErrorAction Stop
            Write-Host "  restarted Claude Desktop" -ForegroundColor Green
            return
        }

        Start-Process $exe -ErrorAction Stop
        Write-Host "  restarted Claude Desktop" -ForegroundColor Green
    }
    catch {
        Write-Host "  [警告] Claude Desktop 已汉化，但自动重启失败：$($_.Exception.Message)" -ForegroundColor DarkYellow
        Write-Host "  [提示] 请从开始菜单手动打开 Claude。" -ForegroundColor DarkYellow
    }
}
"#;

const WINDOWSAPPS_FIND_CLAUDE_PATH: &str = r#"
function Find-ClaudePath {
    $fallback = Get-ChildItem "C:\Program Files\WindowsApps\Claude_*" -Directory -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($fallback) {
        return $fallback.FullName
    }

    return $null
}
"#;

const WINDOWSAPPS_GET_CLAUDE_CONFIG_PATHS: &str = r#"
function Get-ClaudeConfigPaths {
    if (-not $env:LOCALAPPDATA) {
        return @()
    }

    $configPaths = @()
    if ($env:APPDATA) {
        $configPaths += Join-Path $env:APPDATA "Claude\config.json"
        $configPaths += Join-Path $env:APPDATA "Claude-3p\config.json"
    }

    $packageRoot = Join-Path $env:LOCALAPPDATA "Packages"
    $packageDirs = @(Get-ChildItem (Join-Path $packageRoot "Claude_*") -Directory -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending)
    foreach ($packageDir in $packageDirs) {
        $configPaths += Join-Path $packageDir.FullName "LocalCache\Roaming\Claude\config.json"
        $configPaths += Join-Path $packageDir.FullName "LocalCache\Roaming\Claude-3p\config.json"
    }

    return @($configPaths | Select-Object -Unique)
}
"#;

const WINDOWSAPPS_SAFE_UNINSTALL_FUNCTION: &str = r#"
function Uninstall-WindowsLanguagePack {
    Write-Host "=== Claude Desktop Windows 中文补丁卸载 ===" -ForegroundColor Cyan

    $oldSkipAsarPatch = $SkipAsarPatch
    $SkipAsarPatch = $true
    try {
        $paths = Get-ClaudeResourcesPath
    }
    finally {
        $SkipAsarPatch = $oldSkipAsarPatch
    }
    $claudePath = $paths["App"]
    $resourcesPath = $paths["Resources"]

    Write-Step "关闭 Claude Desktop"
    Stop-ClaudeProcesses
    Remove-LegacyAppxForkArtifacts

    Write-Step "[1/4] 恢复前端 bundle 和 app.asar"
    Restore-LatestBackup $resourcesPath
    if (Test-AsarPatchEnabled) {
        Sync-ClaudeExeAsarIntegrity $resourcesPath
    }
    else {
        Write-Host "  skipping Claude.exe asar integrity sync due to patch mode: $PatchMode" -ForegroundColor DarkYellow
    }

    Write-Step "[2/4] 删除中文资源"
    Remove-LanguageFiles $resourcesPath

    Write-Step "[3/4] 移除中文语言注册"
    Unregister-Language $resourcesPath

    Write-Step "[4/4] 恢复用户语言配置"
    Set-ClaudeLocale "en-US"

    Write-Host ""
    Write-Host "卸载完成。请重启 Claude Desktop 使更改生效。" -ForegroundColor Green
}
"#;

fn write_powershell_script(path: &Path, content: &str) -> std::io::Result<()> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(content.trim_start_matches('\u{feff}').as_bytes());
    fs::write(path, bytes)
}

fn launch_claude_from_path(exe: &Path) -> std::io::Result<()> {
    if let Some(app_id) = windowsapps_app_user_model_id(exe) {
        let mut command = Command::new("explorer.exe");
        hide_console_window(&mut command);
        command
            .arg(format!(r"shell:AppsFolder\{app_id}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())?;
        return Ok(());
    }

    let mut command = Command::new(exe);
    hide_console_window(&mut command);
    command
        .current_dir(exe.parent().unwrap_or_else(|| Path::new(".")))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn windowsapps_app_user_model_id(exe: &Path) -> Option<String> {
    let package_root = exe.ancestors().find(|path| {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("WindowsApps"))
    })?;
    let package_name = package_root.file_name()?.to_str()?;
    let (identity, publisher) = package_name.split_once("__")?;
    let app_name = identity.split('_').next()?;
    if app_name.is_empty() || publisher.is_empty() {
        return None;
    }
    Some(format!("{app_name}_{publisher}!{app_name}"))
}

fn current_user_sid() -> Option<String> {
    let mut command = Command::new("powershell.exe");
    hide_console_window(&mut command);
    let output = command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        ])
        .output()
        .ok()?;
    let sid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sid.is_empty() {
        None
    } else {
        Some(sid)
    }
}

fn read_patch_engine_log(engine: &Path) -> String {
    read_text_if_present(&engine.join("install-windows.log"))
}

fn read_text_if_present(path: &Path) -> String {
    decode_text_file(path).trim().to_string()
}

fn decode_text_file(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return String::new();
    };
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_bytes(&bytes[2..], false);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_bytes(&bytes[2..], true);
    }
    if looks_like_utf16_le(&bytes) {
        return decode_utf16_bytes(&bytes, false);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && bytes
            .iter()
            .skip(1)
            .step_by(2)
            .take(32)
            .filter(|byte| **byte == 0)
            .count()
            >= 2
}

fn decode_utf16_bytes(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|chunk| {
        if big_endian {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_le_bytes([chunk[0], chunk[1]])
        }
    });
    String::from_utf16_lossy(&units.collect::<Vec<_>>())
}

fn combined_patch_engine_log(engine: &Path, stdout_log: &Path, stderr_log: &Path) -> String {
    [
        read_patch_engine_log(engine),
        read_text_if_present(stdout_log),
        read_text_if_present(stderr_log),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn process_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stdout.is_empty() {
        return stderr;
    }
    if stderr.is_empty() {
        return stdout;
    }
    format!("{stdout}\n{stderr}")
}

fn tail_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let tail = text
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    format!("... 已省略前面部分日志 ...\n{tail}")
}

fn ps_string(value: impl AsRef<str>) -> String {
    format!("'{}'", value.as_ref().replace('\'', "''"))
}

fn ps_path(path: &Path) -> String {
    ps_string(path.to_string_lossy())
}

fn patch_engine_path() -> PathBuf {
    local_app_data()
        .join(PRODUCT_DATA_DIR)
        .join(PATCH_ENGINE_DIR)
}

fn patch_engine_script_path(engine: &Path) -> PathBuf {
    engine.join("scripts").join("install_windows.ps1")
}

fn claude_exe_path() -> Option<PathBuf> {
    for candidate in claude_exe_candidates() {
        if candidate.exists() {
            return candidate.canonicalize().ok().or(Some(candidate));
        }
    }
    None
}

fn claude_resources_path() -> Option<PathBuf> {
    let exe = claude_exe_path()?;
    for ancestor in exe.ancestors() {
        let resources = ancestor.join("resources");
        if resources.join("app.asar").exists() || resources.join("ion-dist").exists() {
            return Some(resources);
        }
    }
    None
}

fn claude_exe_candidates() -> Vec<PathBuf> {
    let mut candidates = windowsapps_claude_exe_candidates();
    candidates.extend([
        local_app_data()
            .join("Programs")
            .join("Claude")
            .join("Claude.exe"),
        local_app_data().join("AnthropicClaude").join("Claude.exe"),
        local_app_data().join("AnthropicClaude").join("claude.exe"),
        local_app_data().join("Claude").join("Claude.exe"),
        PathBuf::from(r"C:\Program Files\Claude\Claude.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Claude\Claude.exe"),
    ]);

    for app_root in [
        local_app_data().join("AnthropicClaude"),
        local_app_data().join("Claude"),
    ] {
        push_local_app_exe_candidates(&mut candidates, &app_root);
    }

    candidates
}

fn windowsapps_claude_exe_candidates() -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let windows_apps = Path::new(r"C:\Program Files\WindowsApps");
    if let Ok(entries) = fs::read_dir(windows_apps) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.starts_with("Claude_") {
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok();
                packages.push((modified, path));
            }
        }
    }

    packages.sort_by(|left, right| right.0.cmp(&left.0));
    packages
        .into_iter()
        .flat_map(|(_, root)| {
            [
                root.join("app").join("Claude.exe"),
                root.join("app").join("claude.exe"),
                root.join("Claude.exe"),
                root.join("claude.exe"),
            ]
        })
        .collect()
}

fn push_local_app_exe_candidates(candidates: &mut Vec<PathBuf>, app_root: &Path) {
    if let Ok(entries) = fs::read_dir(app_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.starts_with("app-") {
                candidates.push(path.join("Claude.exe"));
                candidates.push(path.join("claude.exe"));
            }
        }
    }
}

fn has_windowsapps_claude() -> bool {
    windowsapps_claude_exe_candidates()
        .iter()
        .any(|path| path.exists())
}

fn claude_install_missing_message() -> &'static str {
    if has_windowsapps_claude() {
        "未能读取 WindowsApps 下的 Claude Desktop，请以管理员权限运行本工具。"
    } else {
        "未找到 Claude Desktop，请先安装 Microsoft Store / 官方 Windows 版。"
    }
}

fn has_zh_resources(resources_path: &Path) -> bool {
    [
        resources_path
            .join("ion-dist")
            .join("i18n")
            .join("zh-CN.json"),
        resources_path
            .join("ion-dist")
            .join("i18n")
            .join("zh-TW.json"),
        resources_path
            .join("ion-dist")
            .join("i18n")
            .join("zh-HK.json"),
        resources_path.join("zh-CN.json"),
        resources_path.join("zh-TW.json"),
        resources_path.join("zh-HK.json"),
    ]
    .iter()
    .any(|path| path.exists())
}

fn backup_root(resources_path: &Path) -> PathBuf {
    resources_path.join(".zh-cn-backups")
}

fn read_claude_locale() -> Option<String> {
    let config = roaming_app_data().join("Claude").join("config.json");
    let text = fs::read_to_string(config).ok()?;
    for locale in ["zh-CN", "zh-TW", "zh-HK", "en-US"] {
        if text.contains(locale) {
            return Some(locale.to_string());
        }
    }
    None
}

fn query_winget_metadata() -> Result<WingetMetadata, String> {
    let mut command = Command::new("winget");
    hide_console_window(&mut command);
    let output = command
        .args([
            "show",
            "--id",
            "Anthropic.Claude",
            "--source",
            "winget",
            "--disable-interactivity",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("执行 winget 失败：{error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(if stderr.is_empty() {
            "winget 查询失败。".to_string()
        } else {
            stderr
        });
    }

    let version = find_winget_value(&stdout, &["版本", "Version"])
        .ok_or_else(|| "winget 输出中没有版本号。".to_string())?;
    let installer_url =
        find_winget_value(&stdout, &["安装程序 URL", "Installer URL", "Installer Url"]);
    let sha256 = find_winget_value(
        &stdout,
        &["安装程序 SHA256", "Installer SHA256", "Installer Sha256"],
    );

    Ok(WingetMetadata {
        version,
        installer_url,
        sha256,
    })
}

fn find_winget_value(output: &str, keys: &[&str]) -> Option<String> {
    for line in output.lines() {
        let Some((raw_key, raw_value)) = line.split_once('：').or_else(|| line.split_once(':'))
        else {
            continue;
        };
        let key = raw_key.trim();
        if keys
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
        {
            let value = raw_value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn update_log(current_version: &str, metadata: &WingetMetadata) -> String {
    let mut lines = vec![
        format!("当前版本：{}", display_version(current_version)),
        format!("最新版本：{}", display_version(&metadata.version)),
        "版本来源：winget Anthropic.Claude".to_string(),
    ];
    if let Some(url) = &metadata.installer_url {
        lines.push(format!("安装程序：{url}"));
    }
    if let Some(sha256) = &metadata.sha256 {
        lines.push(format!("SHA256：{sha256}"));
    }
    lines.join("\n")
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left = version_parts(left);
    let right = version_parts(right);
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_part = *left.get(index).unwrap_or(&0);
        let right_part = *right.get(index).unwrap_or(&0);
        match left_part.cmp(&right_part) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

fn display_version(version: &str) -> String {
    let mut parts = version_parts(version);
    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }
    if parts.is_empty() {
        return version.to_string();
    }
    parts
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

fn version_parts(version: &str) -> Vec<u64> {
    let mut parts = version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect::<Vec<_>>();

    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }

    parts
}

fn command_exists(program: &str) -> bool {
    let mut command = Command::new("where.exe");
    hide_console_window(&mut command);
    command
        .arg(program)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_current_process_elevated() -> bool {
    let mut command = Command::new("powershell.exe");
    hide_console_window(&mut command);
    let output = command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent(); $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
        ])
        .output();

    output
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

fn roaming_app_data() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

fn read_file_version() -> Option<String> {
    let exe = claude_exe_path()?;
    let mut command = Command::new("powershell.exe");
    hide_console_window(&mut command);
    let output = command
        .arg("-NoProfile")
        .arg("-Command")
        .arg(format!(
            "(Get-Item -LiteralPath '{}').VersionInfo.ProductVersion",
            exe.to_string_lossy().replace('\'', "''")
        ))
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

fn hide_console_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn run() {
    if let Some((launcher, completion, stdout_log, stderr_log)) = elevated_helper_args() {
        let code =
            run_elevated_patch_engine_helper(&launcher, &completion, &stdout_log, &stderr_log);
        std::process::exit(code);
    }
    if let Some(result_path) = elevated_check_update_result_path() {
        let code = run_elevated_check_update_helper(&result_path);
        std::process::exit(code);
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            open_claude,
            get_live_log,
            check_update,
            repair,
            install_patch,
            restore_patch,
            set_auto_updates
        ])
        .run(tauri::generate_context!())
        .expect("failed to run app");
}

fn elevated_check_update_result_path() -> Option<PathBuf> {
    let mut args = env::args_os();
    let _ = args.next();
    while let Some(arg) = args.next() {
        if arg == ELEVATED_CHECK_UPDATE_ARG {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

fn elevated_helper_args() -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let mut args = env::args_os();
    let _ = args.next();
    while let Some(arg) = args.next() {
        if arg == ELEVATED_HELPER_ARG {
            let launcher = args.next().map(PathBuf::from)?;
            let completion = args.next().map(PathBuf::from)?;
            let stdout_log = args.next().map(PathBuf::from).unwrap_or_else(|| {
                completion.with_file_name("run-from-claude-desktop-ui.stdout.log")
            });
            let stderr_log = args.next().map(PathBuf::from).unwrap_or_else(|| {
                completion.with_file_name("run-from-claude-desktop-ui.stderr.log")
            });
            return Some((launcher, completion, stdout_log, stderr_log));
        }
    }
    None
}

fn run_elevated_check_update_helper(result_path: &Path) -> i32 {
    let result = check_update_inner();
    let code = if result.ok { 0 } else { 1 };
    let write_result = serde_json::to_vec(&result)
        .map_err(|error| error.to_string())
        .and_then(|bytes| fs::write(result_path, bytes).map_err(|error| error.to_string()));
    if write_result.is_err() {
        return 1;
    }
    code
}

fn run_elevated_patch_engine_helper(
    launcher: &Path,
    completion: &Path,
    stdout_log: &Path,
    stderr_log: &Path,
) -> i32 {
    let mut powershell = Command::new("powershell.exe");
    hide_console_window(&mut powershell);
    let stdout = fs::File::create(stdout_log)
        .ok()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);
    let stderr = fs::File::create(stderr_log)
        .ok()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);
    let status = powershell
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &launcher.to_string_lossy(),
        ])
        .stdout(stdout)
        .stderr(stderr)
        .status();

    let code = status.ok().and_then(|status| status.code()).unwrap_or(1);
    if !completion.exists() {
        let _ = fs::write(completion, code.to_string());
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_localized_winget_values() {
        let output = r#"
已找到 Claude [Anthropic.Claude]
版本: 1.15962.1
安装：
  安装程序 URL： https://downloads.claude.ai/releases/win32/x64/1.15962.1/Claude.exe
  安装程序 SHA256： abc123
"#;

        assert_eq!(
            find_winget_value(output, &["版本", "Version"]).as_deref(),
            Some("1.15962.1")
        );
        assert_eq!(
            find_winget_value(output, &["安装程序 URL", "Installer URL"]).as_deref(),
            Some("https://downloads.claude.ai/releases/win32/x64/1.15962.1/Claude.exe")
        );
        assert_eq!(
            find_winget_value(output, &["安装程序 SHA256", "Installer SHA256"]).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn compares_versions_with_trailing_zero_equivalence() {
        assert_eq!(
            compare_versions("1.15962.1", "1.15962.1.0"),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions("1.15963.0", "1.15962.9"),
            Ordering::Greater
        );
        assert_eq!(compare_versions("1.15962.1", "1.15962.2"), Ordering::Less);
        assert_eq!(display_version("1.15962.1.0"), "1.15962.1");
    }

    #[test]
    fn prepares_windowsapps_script_skips_inner_acl_updates() {
        let root = env::temp_dir().join(format!("cc-desktop-zh-test-{}", std::process::id()));
        let scripts = root.join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(
            scripts.join("install_windows.ps1"),
            "$script:DetectedUnpackagedClaudePaths = @(Get-UnpackagedClaudePaths)\nEnable-WriteAccess $resourcesPath\nfunction Patch-HardcodedFrontendStrings {\n    Write-Host \"slow\"\n}\nfunction Restart-Claude {\n    param([string]$ClaudePath)\n    Start-Process $exe\n}\nfunction Find-ClaudePath {\n    $packages = @(Get-AppxPackage -Name \"Claude\" -ErrorAction SilentlyContinue)\n    return $packages[0].InstallLocation\n}\nfunction Get-ClaudeConfigPaths {\n    $packages = @(Get-AppxPackage -Name \"Claude\" -ErrorAction SilentlyContinue)\n    return @($packages[0].PackageFamilyName)\n}\nfunction Uninstall-WindowsLanguagePack {\n    Restore-LatestBackup $resourcesPath\n    Sync-ClaudeExeAsarIntegrity $resourcesPath\n}\n",
        )
        .unwrap();

        let patched = prepare_patch_engine_appx_script(&root).unwrap();
        let patched_source = fs::read_to_string(patched).unwrap();

        assert!(patched_source.contains("$script:DetectedUnpackagedClaudePaths = @()"));
        assert!(!patched_source.contains("Enable-WriteAccess $resourcesPath"));
        assert!(patched_source.contains("跳过脚本内部权限更新"));
        assert!(patched_source.contains("scanning frontend bundles"));
        assert!(patched_source.contains("shell:AppsFolder\\$appId"));
        assert!(patched_source.contains("自动重启失败"));
        assert!(patched_source.contains(r#"Get-ChildItem "C:\Program Files\WindowsApps\Claude_*""#));
        assert!(patched_source.contains(r#"Get-ChildItem (Join-Path $packageRoot "Claude_*")"#));
        assert!(!patched_source.contains("Get-AppxPackage"));
        assert!(patched_source.contains("if (Test-AsarPatchEnabled)"));
        assert!(patched_source
            .contains("skipping Claude.exe asar integrity sync due to patch mode: $PatchMode"));
        assert!(!patched_source.contains("Write-Host \"slow\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn elevated_launcher_grants_windowsapps_app_access() {
        let content = patch_engine_launcher_content(
            Path::new("C:\\Temp\\engine"),
            Path::new("C:\\Temp\\engine\\scripts\\install_windows_force_windowsapps.ps1"),
            Path::new("C:\\Temp\\engine\\run.exitcode"),
            "uninstall",
            "zh-CN",
            "safe",
            "S-1-5-21-1000",
            "C:\\Users\\Test",
            "C:\\Users\\Test\\AppData\\Roaming",
            "C:\\Users\\Test\\AppData\\Local",
        );

        assert!(content.contains("$appPath = Join-Path $packageRoot.FullName 'app'"));
        assert!(content.contains("& takeown.exe /F $appPath /A /R /D Y"));
        assert!(content.contains("& icacls.exe $appPath /grant $grant /T /C /Q"));
        assert!(!content.contains("& icacls.exe $resourcesPath /grant $grant"));
    }

    #[test]
    fn derives_windowsapps_app_user_model_id() {
        let exe = Path::new(
            r"C:\Program Files\WindowsApps\Claude_1.17377.1.0_x64__pzs8sxrjxfjjc\app\claude.exe",
        );

        assert_eq!(
            windowsapps_app_user_model_id(exe).as_deref(),
            Some("Claude_pzs8sxrjxfjjc!Claude")
        );
    }

    #[test]
    fn elevated_launch_command_uses_hidden_gui_helper() {
        let command = elevated_start_command(
            Path::new("C:\\Temp\\tool.exe"),
            "--run-patch-engine-elevated-helper \"C:\\Temp\\run.ps1\" \"C:\\Temp\\done.txt\"",
            Path::new("C:\\Temp"),
        );

        assert!(command.contains("tool.exe"));
        assert!(command.contains(ELEVATED_HELPER_ARG));
        assert!(command.contains("-WindowStyle Hidden"));
        assert!(!command.contains("-FilePath 'powershell.exe'"));
        assert!(!command.contains("-Wait"));
    }

    #[test]
    fn writes_powershell_scripts_with_utf8_bom() {
        let path =
            env::temp_dir().join(format!("cc-desktop-zh-bom-test-{}.ps1", std::process::id()));

        write_powershell_script(&path, "\u{feff}Write-Host \"请选择操作\"").unwrap();
        let bytes = fs::read(&path).unwrap();

        assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(bytes[3], b'W');

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reads_windows_log_encodings() {
        let utf8_path = env::temp_dir().join(format!(
            "cc-desktop-zh-utf8-log-test-{}.log",
            std::process::id()
        ));
        let utf16_path = env::temp_dir().join(format!(
            "cc-desktop-zh-utf16-log-test-{}.log",
            std::process::id()
        ));

        fs::write(
            &utf8_path,
            [b"\xEF\xBB\xBF".as_slice(), "安装完成".as_bytes()].concat(),
        )
        .unwrap();
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "恢复原样".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&utf16_path, utf16).unwrap();

        assert_eq!(read_text_if_present(&utf8_path), "安装完成");
        assert_eq!(read_text_if_present(&utf16_path), "恢复原样");

        let _ = fs::remove_file(utf8_path);
        let _ = fs::remove_file(utf16_path);
    }

    #[test]
    fn parses_bom_prefixed_exit_code() {
        let path = env::temp_dir().join(format!(
            "cc-desktop-zh-exitcode-test-{}.txt",
            std::process::id()
        ));
        fs::write(&path, b"\xEF\xBB\xBF0\r\n").unwrap();

        let code = wait_for_elevated_completion(&path, Duration::from_secs(1)).unwrap();
        assert_eq!(code, 0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn fast_frontend_patch_function_is_powershell_parseable() {
        let path = env::temp_dir().join(format!(
            "cc-desktop-zh-fast-patch-{}.ps1",
            std::process::id()
        ));

        write_powershell_script(&path, FAST_HARDCODED_FRONTEND_PATCH_FUNCTION).unwrap();
        let mut command = Command::new("powershell.exe");
        hide_console_window(&mut command);
        let output = command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &path.to_string_lossy(),
            ])
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn fast_frontend_patch_function_handles_effort_label_variable_changes() {
        let root = env::temp_dir().join(format!(
            "cc-desktop-zh-effort-label-test-{}",
            std::process::id()
        ));
        let assets = root.join("ion-dist").join("assets").join("v1");
        fs::create_dir_all(&assets).unwrap();
        let bundle = assets.join("effort.js");
        fs::write(
            &bundle,
            r#"const nj=["low","medium","high","xhigh","max"];const e=Jt.map(e=>({label:e.name,value:e.id,checked:!hn&&e.id===cn,onSelect:()=>wn(e.id,!1)}));"#,
        )
        .unwrap();

        let script = format!(
            "$ErrorActionPreference='Stop'\n\
             $Utf8NoBom = [System.Text.UTF8Encoding]::new($false)\n\
             function Get-FrontendHardcodedReplacements {{ param([string]$Language) return @() }}\n\
             function Test-StructuralJsReplacement {{ param([string]$Source) return $false }}\n\
             function Test-PlainUiTextReplacement {{ param([string]$Source) return $true }}\n\
             function Backup-ModifiedFile {{ param([string]$ResourcesPath, [string]$FilePath) }}\n\
             {function_body}\n\
             Patch-HardcodedFrontendStrings {root} 'zh-CN'\n\
             $text = [System.IO.File]::ReadAllText({bundle}, [System.Text.Encoding]::UTF8)\n\
             $expected = 'label:({{low:\"低\",medium:\"中\",high:\"高\",xhigh:\"超高\",max:\"最高\"}}[e.id]??e.name),value:e.id,checked:!hn&&e.id===cn,onSelect:()=>wn(e.id,!1)}}'\n\
             if (-not $text.Contains($expected)) {{ throw \"effort label patch did not apply: $text\" }}\n",
            function_body = FAST_HARDCODED_FRONTEND_PATCH_FUNCTION,
            root = ps_path(&root),
            bundle = ps_path(&bundle),
        );
        let script_path = root.with_extension("ps1");
        write_powershell_script(&script_path, &script).unwrap();

        let mut command = Command::new("powershell.exe");
        hide_console_window(&mut command);
        let output = command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.to_string_lossy(),
            ])
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(script_path);
    }
}
