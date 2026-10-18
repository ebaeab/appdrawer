//! 跨平台的“在新窗口 / 新会话中启动命令”。
//!
//! 分派顺序：
//! * **Termux（Android）**：通过 `am startservice` 打开一个新的 Termux 会话（新窗口），
//!   命令结束后进入交互式 shell，窗口保持打开；
//! * **Windows**：默认用 **PowerShell** 执行；简单命令用 `Start-Process` 启动（不留窗口），
//!   复杂命令（cmdlet、管道等）在交互式 PowerShell 窗口中运行；
//! * **macOS**：用 `open -a Terminal <脚本>` 在“终端”里打开；
//! * **Linux/其它 Unix**：依次尝试常见终端模拟器（x-terminal-emulator、gnome-terminal、
//!   konsole、xfce4-terminal、kitty、alacritty、wezterm、foot、xterm…）；
//! * 以上都不可用时，回退为**后台分离运行**（不占用当前 TUI）。
//!
//! 无论哪种方式，AppDrawer 本身都不会退出，可重复启动。

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[cfg(any(unix, windows))]
use std::process::Stdio;

/// Termux 下的 bash 绝对路径（其环境 `PATH` 只含 `$PREFIX/bin`）。
const TERMUX_BASH: &str = "/data/data/com.termux/files/usr/bin/bash";

/// 在新窗口 / 新会话中启动命令，返回可显示的状态描述。
pub fn launch(name: &str, command: &str) -> io::Result<String> {
    // Termux 优先（Android 上唯一可用的“新窗口”方式）。
    if is_termux() {
        return match termux_new_session(name, command) {
            Ok(()) => Ok("已在新会话中启动".to_string()),
            Err(e) => detached_fallback(command, e),
        };
    }

    match launch_window(name, command) {
        Ok(msg) => Ok(msg),
        Err(e) => detached_fallback(command, e),
    }
}

/// 是否为 Termux 环境。
fn is_termux() -> bool {
    std::env::var_os("TERMUX_VERSION").is_some()
}

/// 后台分离运行，并附带失败原因。
fn detached_fallback(command: &str, cause: io::Error) -> io::Result<String> {
    spawn_detached(command)?;
    Ok(format!("已后台运行（{}）", cause))
}

// ---------------------------------------------------------------------------
// 纯函数：各平台的启动脚本
// ---------------------------------------------------------------------------

/// Termux 新会话脚本：命令结束后进入交互式 shell，使新窗口保持打开。
pub fn build_session_script(name: &str, command: &str) -> String {
    build_unix_like_script(name, command, TERMUX_BASH, TERMUX_BASH)
}

/// 通用 Unix 新终端脚本（Linux/macOS）：结束后进入用户登录 shell。
#[allow(dead_code)] // 仅当目标平台为 macOS / Linux 时用于启动。
pub fn build_unix_script(name: &str, command: &str) -> String {
    build_unix_like_script(name, command, "/usr/bin/env bash", "\"${SHELL:-bash}\"")
}

fn build_unix_like_script(name: &str, command: &str, shebang: &str, shell: &str) -> String {
    // 名称只作为 printf 的数据参数（%s）传入；转义引号/换行以免破坏脚本。
    let safe_name = name.replace(['\'', '\n', '\r'], " ");
    format!(
        "#!{shebang}\n\
printf '\\033]0;%s\\007' 'AppDrawer: {name}'\n\
cd \"$HOME\" 2>/dev/null || true\n\
{command}\n\
__appdrawer_code=$?\n\
printf '\\n\\033[2m[AppDrawer] %s 已结束（退出码 %s）。此窗口保持打开，输入 exit 可关闭。\\033[0m\\n' '「{name}」' \"$__appdrawer_code\"\n\
exec {shell} -i\n",
        shebang = shebang,
        shell = shell,
        name = safe_name,
        command = command,
    )
}

/// 判断命令是否可以交给 `Start-Process` 直接启动（即“单个程序 + 参数”）。
///
/// 含 PowerShell 运算符 / 变量 / 分组，或首词是 cmdlet（`Verb-Noun`）/常用别名时返回
/// `false`——这些必须由 PowerShell 自己执行。
pub fn is_simple_command(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    if command.chars().any(|c| {
        matches!(
            c,
            '&' | '|' | ';' | '<' | '>' | '(' | ')' | '{' | '}' | '$' | '`' | '@' | '#'
                | '\n' | '\r'
        )
    }) {
        return false;
    }
    let first = command.split_whitespace().next().unwrap_or("");
    let first = first.trim_matches(['"', '\'']);
    if is_powershell_cmdlet(first) {
        return false;
    }
    let stem = std::path::Path::new(first)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(first)
        .to_ascii_lowercase();
    !matches!(
        stem.as_str(),
        "echo"
            | "set"
            | "cd"
            | "chdir"
            | "dir"
            | "ls"
            | "cat"
            | "type"
            | "cp"
            | "copy"
            | "mv"
            | "move"
            | "rm"
            | "del"
            | "erase"
            | "pwd"
            | "ps"
            | "kill"
            | "cls"
            | "clear"
            | "clc"
            | "sleep"
            | "where"
            | "select"
            | "sort"
            | "measure"
            | "history"
            | "man"
            | "date"
            | "gc"
            | "gci"
            | "gcm"
            | "gi"
            | "gl"
            | "gp"
            | "gps"
            | "gv"
            | "gwmi"
            | "sl"
            | "sv"
            | "ni"
            | "si"
            | "ri"
            | "ci"
            | "mi"
            | "start"
            | "iex"
            | "icm"
    )
}

/// 首词是否形如 PowerShell cmdlet（`Verb-Noun`）。
fn is_powershell_cmdlet(token: &str) -> bool {
    let mut parts = token.splitn(2, '-');
    match (parts.next(), parts.next()) {
        (Some(verb), Some(noun)) => {
            !verb.is_empty()
                && !noun.is_empty()
                && verb.chars().all(|c| c.is_ascii_alphabetic())
                && noun.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

/// 按双引号规则拆分命令：第一段是程序，其余是参数。
fn split_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut started = false;
    for c in command.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                started = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if started {
                    tokens.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        tokens.push(cur);
    }
    tokens
}

/// PowerShell 单引号字符串转义。
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Windows 启动脚本（PowerShell）。
///
/// * 以 UTF-8 **BOM** 写出，保证 Windows PowerShell 5.1 也能正确读取中文；
/// * 简单命令（单个外部程序）用 `Start-Process` 启动，脚本随即退出，**不残留窗口**；
/// * 复杂命令（cmdlet、管道、变量等）交给 PowerShell 执行，窗口保持打开（交互式），
///   方便查看输出。
#[allow(dead_code)] // 仅当目标平台为 Windows 时用于启动。
pub fn build_windows_script(command: &str) -> String {
    let launch = if is_simple_command(command) {
        let tokens = split_command(command);
        let program = tokens.first().map(String::as_str).unwrap_or("");
        let args = tokens.get(1..).unwrap_or(&[]).join(" ");
        if args.is_empty() {
            format!("Start-Process -FilePath {}", ps_quote(program))
        } else {
            format!(
                "Start-Process -FilePath {} -ArgumentList {}",
                ps_quote(program),
                ps_quote(&args)
            )
        }
    } else {
        command.to_string()
    };
    format!(
        "\u{FEFF}$ErrorActionPreference = 'Continue'\r\nSet-Location -LiteralPath $HOME\r\n{launch}\r\n"
    )
}

// ---------------------------------------------------------------------------
// 平台脚本落盘
// ---------------------------------------------------------------------------

/// 把脚本写入 `$TMPDIR/appdrawer-sessions/`，可选设置为可执行。
fn write_script(ext: &str, contents: &str, executable: bool) -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("appdrawer-sessions");
    fs::create_dir_all(&dir)?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("session-{}-{}.{}", std::process::id(), stamp, ext));
    fs::write(&path, contents)?;

    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&path, perms)?;
    }
    #[cfg(not(unix))]
    let _ = executable;

    Ok(path)
}

// ---------------------------------------------------------------------------
// Termux
// ---------------------------------------------------------------------------

fn termux_new_session(name: &str, command: &str) -> io::Result<()> {
    let script_path = write_script("sh", &build_session_script(name, command), true)?;
    let script = script_path
        .to_str()
        .ok_or_else(|| io::Error::other("临时脚本路径非法"))?;

    let output = Command::new("am")
        .args([
            "startservice",
            "--user",
            "current",
            "-n",
            "com.termux/com.termux.app.TermuxService",
            "-a",
            "com.termux.service_execute",
            "-d",
            script,
            "--ez",
            "com.termux.execute.background",
            "false",
        ])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// 各平台“新窗口”实现
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn launch_window(name: &str, command: &str) -> io::Result<String> {
    windows_new_console(name, command)?;
    Ok(if is_simple_command(command) {
        "已启动".to_string()
    } else {
        "已在 PowerShell 中启动".to_string()
    })
}

#[cfg(target_os = "macos")]
fn launch_window(name: &str, command: &str) -> io::Result<String> {
    macos_new_window(name, command).map(|()| "已在新窗口中启动".to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn launch_window(name: &str, command: &str) -> io::Result<String> {
    unix_new_terminal(name, command).map(|prog| format!("已在新窗口（{}）中启动", prog))
}

#[cfg(not(any(windows, target_os = "macos", all(unix, not(target_os = "macos")))))]
fn launch_window(_name: &str, _command: &str) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Other,
        "当前平台没有可用的窗口启动方式",
    ))
}

#[cfg(windows)]
fn windows_new_console(_name: &str, command: &str) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let simple = is_simple_command(command);
    let script = write_script("ps1", &build_windows_script(command), false)?;

    let mut ps = Command::new(powershell_exe());
    ps.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass");
    if simple {
        // 程序由脚本内的 Start-Process 自行开窗，这里不开控制台。
        ps.creation_flags(CREATE_NO_WINDOW);
    } else {
        // 交互式 PowerShell 窗口，命令跑完后仍可查看输出。
        ps.arg("-NoExit").creation_flags(CREATE_NEW_CONSOLE);
    }
    ps.arg("-File").arg(&script).spawn()?;
    Ok(())
}

/// 选择 PowerShell 解释器：优先 PowerShell 7（`pwsh`），否则用内置的 `powershell`。
#[cfg(windows)]
fn powershell_exe() -> &'static str {
    let has_pwsh = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join("pwsh.exe").is_file()))
        .unwrap_or(false);
    if has_pwsh { "pwsh" } else { "powershell" }
}

// ---------------------------------------------------------------------------
// Windows 控制台体验
// ---------------------------------------------------------------------------

/// Windows：把本进程标为 DPI 感知。
///
/// 不做的话，在 125% / 150% 缩放的屏幕上 `GetCursorPos`、显示器工作区、
/// `GetDeviceCaps` 都会被系统虚拟化，而 wezterm 的 `--position` 和 `SetWindowPos`
/// 用的都是物理像素——两者对不上，居中会偏、拖动会跟不上鼠标。
pub fn prepare_dpi() {
    #[cfg(windows)]
    screen::set_dpi_aware();
}

/// 旧版控制台（双击 exe 打开的 conhost）无法渲染彩色 emoji；
/// 若检测到 Windows Terminal（`wt.exe`）且当前不在其中，就在其中重新启动自己。
///
/// `count` 是应用数量，用来估算一个手机抽屉大小的紧凑窗口。
/// 返回 `true` 表示已在新终端中重启，调用方应立即退出当前进程。
/// 可用环境变量 `APPDRAWER_NO_WT=1` 禁用该行为，
/// `APPDRAWER_WT_FULLSCREEN=1` 让新窗口全屏（占满屏幕、没有标题栏）。
pub fn relaunch_in_modern_terminal(count: usize) -> bool {
    // “永远不要换窗口”的总开关（跟 NO_WT 区分开：NO_WT 只管 Windows Terminal 那一步）。
    if std::env::var_os("APPDRAWER_NO_RELAUNCH").is_some() {
        crate::log::log("relaunch: 跳过（APPDRAWER_NO_RELAUNCH 已设置）");
        return false;
    }

    // 防止无限重启：已经在我们开出来的无边框终端里了。
    if std::env::var_os("APPDRAWER_IN_BORDERLESS").is_some() {
        crate::log::log("relaunch: 跳过（已在无边框终端中运行）");
        return false;
    }

    // 无边框：Windows Terminal 做不到，改用 WezTerm / Alacritty 这类支持
    // `decorations = "none"` 的终端自己开一个窗口。
    if borderless_requested() {
        if is_termux() {
            crate::log::log(
                "relaunch: 无边框请求被忽略（Termux 的窗口由 Android 管理，请用 Termux 设置里的“全屏”）",
            );
        } else if borderless_relaunch(count) {
            return true;
        }
    }

    #[cfg(windows)]
    return windows_relaunch(count);
    #[cfg(not(windows))]
    let _ = count;
    #[cfg(not(windows))]
    crate::log::log("relaunch: 非 Windows 平台，跳过");
    #[cfg(not(windows))]
    return false;
}

/// 按“手机抽屉”布局估算一个紧凑的终端窗口尺寸，返回 `(列, 行)`。
///
/// 4 列手机竖屏网格 + 顶栏 + 命令预览 + 底栏，行数随应用数量增长。
/// Windows Terminal 用 `wt.exe --size <列>,<行>` 拿这个值开一个刚好合适的窗口。
pub fn compact_window_size(count: usize) -> (u16, u16) {
    const COLUMNS: usize = 4; // 手机竖屏观感
    const CELL_W: usize = 9;
    let cols = count.max(1).div_ceil(COLUMNS).min(32);
    // 网格宽 + 左右各 1 列留白
    let width = (COLUMNS * CELL_W + (COLUMNS - 1) + 2) as u16;
    // 顶栏 1 + 命令预览 1 + 网格 rows*2 + 底栏 1 + 1 行余量
    let height = (cols * 2 + 5).clamp(13, 45) as u16;
    (width, height)
}

#[cfg(windows)]
fn windows_relaunch(count: usize) -> bool {
    use std::process::Command;

    // 默认开启：旧版控制台无法渲染 emoji，能跳就跳。
    // 设 APPDRAWER_NO_WT=1 或 APPDRAWER_USE_WT=0 可关闭。
    if std::env::var_os("APPDRAWER_NO_WT").is_some() {
        crate::log::log("relaunch: 跳过（APPDRAWER_NO_WT 已设置）");
        return false;
    }
    match std::env::var("APPDRAWER_USE_WT") {
        Ok(v) if matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off") => {
            crate::log::log("relaunch: 跳过（APPDRAWER_USE_WT 为假）");
            return false;
        }
        _ => {}
    }
    if std::env::var_os("WT_SESSION").is_some() {
        crate::log::log("relaunch: 跳过（已在 Windows Terminal 中，WT_SESSION 已设置）");
        return false;
    }
    if std::env::var_os("APPDRAWER_IN_WT").is_some() {
        crate::log::log("relaunch: 跳过（APPDRAWER_IN_WT 已设置）");
        return false;
    }
    // 已在其它已知的现代终端里，也不折腾。
    for key in [
        "ConEmuANSI",
        "ConEmuPID",
        "WEZTERM_EXECUTABLE",
        "ALACRITTY_LOG",
        "VSCODE_INJECTION",
    ] {
        if std::env::var_os(key).is_some() {
            crate::log::log(&format!("relaunch: 跳过（检测到现代终端 {key}）"));
            return false;
        }
    }

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            crate::log::log(&format!("relaunch: 无法获取当前程序路径: {e}"));
            return false;
        }
    };
    let cwd = std::env::current_dir().ok();
    crate::log::log(&format!("relaunch: 开始尝试启动 wt.exe，exe={exe:?}"));

    // 不预先判断 wt.exe 是否存在：WindowsApps 里它通常是 App Execution Alias
    // （重解析点），`is_file()` / `exists()` 会误判。逐个候选真正 spawn，
    // 只有 spawn 成功才算“启动成功”。
    for wt in wt_candidates(std::env::var_os("PATH").as_deref()) {
        let mut cmd = Command::new(&wt);
        // 窗口大小：默认开一个手机抽屉大小的紧凑窗口，避免满屏空白。
        // macOS / Linux 的终端模拟器参数各异，这里只管 Windows Terminal。
        let fullscreen = std::env::var("APPDRAWER_WT_FULLSCREEN")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if fullscreen {
            // 全屏：没有标题栏，屏幕上只剩抽屉本体。
            cmd.arg("--fullscreen");
            crate::log::log("relaunch: 使用 --fullscreen");
        } else {
            let (cols, rows) = compact_window_size(count);
            cmd.arg("--size").arg(format!("{cols},{rows}"));
            crate::log::log(&format!("relaunch: 窗口尺寸 {cols}x{rows}"));
        }
        cmd.arg(&exe);
        // 标记：即使 WT 没有设置 WT_SESSION，也能防止无限重启。
        cmd.env("APPDRAWER_IN_WT", "1");
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        match cmd.spawn() {
            Ok(child) => {
                crate::log::log(&format!("relaunch: 成功启动 {wt:?} (pid={})", child.id()));
                return true;
            }
            Err(e) => {
                crate::log::log(&format!("relaunch: 启动失败 {wt:?}: {e}"));
            }
        }
    }
    crate::log::log("relaunch: 所有 wt.exe 候选均失败");
    false
}

// ---------------------------------------------------------------------------
// 无边框窗口
// ---------------------------------------------------------------------------
//
// Windows Terminal 没有任何“去掉标题栏 / 无边框”的参数（只有 --maximized /
// --fullscreen / --size / --pos），所以无边框模式改用下面两类**本来就有**
// decorations 开关的终端：
//
// * WezTerm：`window_decorations="NONE"`，纯命令行就能设，跨平台。
// * Alacritty：`[window] decorations = "none"`，需要一个临时配置文件。
//
// 都没有时自动降级回 Windows Terminal（有边框，但 emoji 正常）。

/// 是否要开无边框窗口。
///
/// **Windows 上默认开启**（双击 exe 就是无边框），其它平台默认关闭（需显式设环境变量），
/// 因为 Linux/macOS 下通常是由用户自己的终端启动的，不该自作主张再开一个窗。
///
/// 取值：`0/false/no/off` 关闭；`1/true/yes/on` 完全无边框（`NONE`）；
/// `resize` 保留可拖拽缩放的隐形边框（`RESIZE`，仍然没有标题栏）。
pub fn borderless_requested() -> bool {
    // 没设这个变量时：Windows 默认开（双击就想要无边框），其它平台关。
    env_flag("APPDRAWER_BORDERLESS").unwrap_or(cfg!(windows))
}

/// 读取布尔型环境变量：`1/true/yes/on` 为真，`0/false/no/off` 为假。
fn env_flag(name: &str) -> Option<bool> {
    let v = std::env::var(name).ok()?;
    match v.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "resize" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// WezTerm 的窗口装饰模式：`APPDRAWER_BORDERLESS=resize` 时保留缩放边框。
fn decorations_mode() -> &'static str {
    match std::env::var("APPDRAWER_BORDERLESS")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "resize" => "RESIZE",
        _ => "NONE",
    }
}

// ---------------------------------------------------------------------------
// 居中：屏幕 / 显示器工作区 → 窗口像素坐标
// ---------------------------------------------------------------------------

/// 估算一个字符单元的像素尺寸。
///
/// WezTerm 默认字号 12pt，等宽字体字宽 ≈ 0.6 em，行高 ≈ 1.2 em。
/// 只是估算（拿不到终端的真实字体度量），误差几像素，中心基本吻合。
#[cfg(any(windows, test))]
fn cell_px(font_px: f64) -> (f64, f64) {
    (font_px * 0.6, font_px * 1.2)
}

/// 把 `cols x rows` 的窗口居中放进 `(ax, ay, aw, ah)` 这个工作区。
/// 纯函数，方便单元测试；窗口比屏幕还大时就贴在左上角。
#[cfg(any(windows, test))]
pub fn centered_in(
    ax: i32,
    ay: i32,
    aw: i32,
    ah: i32,
    cols: u16,
    rows: u16,
    font_px: f64,
) -> (i32, i32) {
    let (cw, ch) = cell_px(font_px);
    let w = (cols as f64 * cw).round() as i32;
    let h = (rows as f64 * ch).round() as i32;
    (
        ax + ((aw - w) / 2).max(0),
        ay + ((ah - h) / 2).max(0),
    )
}

/// 解析 `APPDRAWER_WINDOW_POS=100,200`。
pub fn parse_pos(s: &str) -> Option<(i32, i32)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(windows)]
mod screen {
    #[repr(C)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }
    #[repr(C)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
    #[repr(C)]
    pub struct MonitorInfo {
        pub cb_size: u32,
        pub rc_monitor: Rect,
        pub rc_work: Rect,
        pub flags: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn GetCursorPos(p: *mut Point) -> i32;
        fn MonitorFromPoint(pt: Point, flags: u32) -> *mut core::ffi::c_void;
        fn GetMonitorInfoW(h: *mut core::ffi::c_void, mi: *mut MonitorInfo) -> i32;
        fn GetDC(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn ReleaseDC(hwnd: *mut core::ffi::c_void, hdc: *mut core::ffi::c_void) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetDeviceCaps(hdc: *mut core::ffi::c_void, index: i32) -> i32;
    }

    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    const LOGPIXELSY: i32 = 90;

    /// 让本进程按**物理像素**工作。
    ///
    /// 不做这一步的话，在 125% / 150% 缩放的屏幕上，系统会把
    /// `GetCursorPos` / 工作区 / DPI 全部按比例虚拟化，而我们传给 wezterm 的
    /// `--position` 是物理像素——两者对不上，窗口会偏到左上角去。
    pub fn set_dpi_aware() {
        unsafe {
            SetProcessDPIAware();
        }
    }

    /// 鼠标所在显示器的“工作区”（不含任务栏）。
    ///
    /// 双击 exe 时鼠标就在想开窗的那块屏上，所以拿它而不是主屏。
    pub fn work_area() -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mut pt = Point { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            let mon = MonitorFromPoint(Point { x: pt.x, y: pt.y }, MONITOR_DEFAULTTONEAREST);
            if mon.is_null() {
                return None;
            }
            let mut mi: MonitorInfo = std::mem::zeroed();
            mi.cb_size = std::mem::size_of::<MonitorInfo>() as u32;
            if GetMonitorInfoW(mon, &mut mi) == 0 {
                return None;
            }
            let r = mi.rc_work;
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            if w > 0 && h > 0 {
                Some((r.left, r.top, w, h))
            } else {
                None
            }
        }
    }

    /// 系统 DPI（96 = 100%）。用老掉牙的 GetDeviceCaps，Win7 也能用。
    pub fn dpi() -> f64 {
        unsafe {
            let hdc = GetDC(std::ptr::null_mut());
            if hdc.is_null() {
                return 96.0;
            }
            let d = GetDeviceCaps(hdc, LOGPIXELSY);
            ReleaseDC(std::ptr::null_mut(), hdc);
            if d > 0 { d as f64 } else { 96.0 }
        }
    }
}

/// 窗口该放在屏幕的哪个像素位置。
///
/// 1. `APPDRAWER_WINDOW_POS=x,y` 手动指定；
/// 2. Windows：按鼠标所在显示器的工作区居中；
/// 3. 其它平台：不指定，交给窗口管理器。
pub fn window_position(cols: u16, rows: u16) -> Option<(i32, i32)> {
    if let Ok(v) = std::env::var("APPDRAWER_WINDOW_POS") {
        match parse_pos(&v) {
            Some(pos) => {
                crate::log::log(&format!(
                    "window: 用 APPDRAWER_WINDOW_POS 指定位置 {pos:?}"
                ));
                return Some(pos);
            }
            None => crate::log::log(&format!(
                "window: APPDRAWER_WINDOW_POS={v:?} 解析失败（要写成 100,200）"
            )),
        }
    }
    #[cfg(windows)]
    {
        screen::set_dpi_aware();
        if let Some((ax, ay, aw, ah)) = screen::work_area() {
            let dpi = screen::dpi();
            let font_px = dpi / 6.0; // 12pt：px = pt * dpi / 72
            let pos = centered_in(ax, ay, aw, ah, cols, rows, font_px);
            crate::log::log(&format!(
                "window: 居中到 {pos:?}（工作区 {aw}x{ah}+{ax}+{ay}，DPI {dpi:.0}，字号约 {font_px:.1}px）"
            ));
            return Some(pos);
        }
        crate::log::log("window: 取不到显示器工作区，位置交给系统");
    }
    let _ = (cols, rows);
    None
}

/// Alacritty 配置文件（TOML）：去掉装饰、按网格尺寸开窗、直接跑本程序。
pub fn alacritty_config(
    exe: &std::path::Path,
    cols: u16,
    rows: u16,
    pos: Option<(i32, i32)>,
) -> String {
    // Alacritty 的 position 是老版本也认的 {x,y} 写法（比 "Centered" 兼容）
    let position = match pos {
        Some((x, y)) => format!("position = {{ x = {x}, y = {y} }}\n"),
        None => String::new(),
    };
    // 亮色主题下把终端背景也改成界面底色：内边距 / 取整剩下的像素是终端画的，
    // 不覆盖的话会在亮色界面周围留一道深色边。
    let colors = match crate::theme::bg_hex() {
        Some(bg) => format!("\n[colors.primary]\nbackground = '{bg}'\n"),
        None => String::new(),
    };
    format!(
        "# AppDrawer 自动生成：无边框窗口（可随时删除）\n\
         [window]\n\
         decorations = \"none\"\n\
         padding = {{ x = 0, y = 0 }}\n\
         {position}\
         dimensions = {{ columns = {cols}, lines = {rows} }}\n\
         {colors}\n\
         [terminal]\n\
         shell = {{ program = \"{}\", args = [] }}\n",
        toml_escape(&exe.to_string_lossy())
    )
}

/// TOML 字符串里的转义（Windows 路径全是反斜杠）。
fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 生成无边框终端的候选路径，按可信度排序：
///
/// 1. 跟 `appdrawer.exe` **放在同一个目录**的免安装版（直接解压到旁边就能用）；
/// 2. `PATH` 里的同名文件；
/// 3. 常见安装位置（Program Files、`%LOCALAPPDATA%\Programs`、Scoop、Chocolatey、
///    Microsoft Store 的 `WindowsApps`）——只收真实存在的，避免无意义地 spawn；
/// 4. 裸名字（交给系统解析）。
fn borderless_candidates(file_name: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();

    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        let p = dir.join(file_name);
        if p.exists() {
            v.push(p);
        }
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let p = dir.join(file_name);
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }

    for dir in known_install_dirs() {
        let p = dir.join(file_name);
        if p.exists() && !v.contains(&p) {
            v.push(p);
        }
    }

    let bare = PathBuf::from(file_name);
    if !v.contains(&bare) {
        v.push(bare);
    }
    v
}

/// 常见安装目录（不存在也无所谓，调用方会再筛）。
fn known_install_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "LOCALAPPDATA",
        "APPDATA",
        "ProgramData",
        "USERPROFILE",
    ] {
        let Some(base) = std::env::var_os(var) else {
            continue;
        };
        let base = PathBuf::from(base);
        v.push(base.join("WezTerm"));
        v.push(base.join("Alacritty"));
        v.push(base.join("Programs").join("WezTerm"));
        v.push(base.join("Programs").join("Alacritty"));
        v.push(base.join("Microsoft").join("WindowsApps"));
        v.push(base.join("scoop").join("shims"));
        v.push(base.join("scoop").join("apps").join("wezterm").join("current"));
        v.push(
            base.join("scoop")
                .join("apps")
                .join("alacritty")
                .join("current"),
        );
    }
    v.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
    v
}

/// Windows 上让子进程**不共用我们的控制台**：
/// 否则我们这个（双击产生的）控制台一关闭，子进程会收到 `CTRL_CLOSE_EVENT`，
/// 容易出现“窗口刚起来又没了”。`CREATE_NO_WINDOW` = 给它一个隐藏控制台。
fn detach_from_console(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        _cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// 一次 spawn 尝试的结果。
enum SpawnOutcome {
    /// 进程活着（窗口应该已经开了）
    Running(std::process::Child),
    /// spawn 成功、但进程立刻退出 —— 绝大多数是“这个版本不认这些参数”
    Died(String),
    /// 根本没能启动：路径不存在 / 没有权限
    Missing,
}

/// 启动一个无边框终端，并把它 的 stderr 收进临时文件。
/// 子进程失败时，日志里就能看到它自己喊的错因，而不是只有“没启动”。
fn spawn_quietly(prog: &std::path::Path, args: &[String], cwd: Option<&PathBuf>) -> SpawnOutcome {
    let err_path = std::env::temp_dir()
        .join("appdrawer-sessions")
        .join("borderless-stderr.log");
    let _ = fs::create_dir_all(err_path.parent().unwrap_or(std::path::Path::new(".")));
    let stderr = fs::File::create(&err_path).ok();

    let mut cmd = Command::new(prog);
    cmd.args(args).env("APPDRAWER_IN_BORDERLESS", "1");
    // 不要把子进程的输出拌回我们正在用的备用屏幕
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    match stderr {
        Some(f) => {
            cmd.stderr(std::process::Stdio::from(f));
        }
        None => {
            cmd.stderr(std::process::Stdio::null());
        }
    }
    detach_from_console(&mut cmd);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // “找不到”是常态（候选列表很长）；只有“文件在、但起不来”才值得记
            if e.kind() != io::ErrorKind::NotFound && prog.exists() {
                crate::log::log(&format!("borderless: 启动 {prog:?} 失败: {e}"));
            }
            return SpawnOutcome::Missing;
        }
    };

    // 观察片刻：窗口类进程会一直活着；参数不被接受的话会立刻退出。
    std::thread::sleep(std::time::Duration::from_millis(300));
    match child.try_wait() {
        Ok(Some(status)) if !status.success() => {
            let msg: String = fs::read_to_string(&err_path)
                .unwrap_or_default()
                .trim()
                .chars()
                .take(400)
                .collect();
            let msg = if msg.is_empty() {
                format!("（stderr 无输出，文件：{}）", err_path.display())
            } else {
                format!("：{msg}")
            };
            SpawnOutcome::Died(format!("{status}{msg}"))
        }
        _ => SpawnOutcome::Running(child),
    }
}

/// WezTerm 的几种参数写法：从“最完整”到“最保守”。
///
/// 不同版本对 `--` 分隔符、以及 `initial_cols` / `initial_rows` 这类配置项的
/// 支持不完全一致；一旦某种写法让 wezterm 直接退出，就自动换下一种。
pub fn wezterm_arg_variants(
    exe: &std::path::Path,
    cols: u16,
    rows: u16,
    dec: &str,
    pos: Option<(i32, i32)>,
) -> Vec<Vec<String>> {
    let exe_s = exe.to_string_lossy().into_owned();
    let full = wezterm_opts(cols, rows, dec);
    let only_dec = wezterm_plain_opts(dec, true);
    // 有些版本把无法解析为 TOML 的值当普通字符串，所以不带引号也试一下
    let bare_dec = wezterm_plain_opts(dec, false);
    vec![
        // #1 推荐：全局 --config + 居中 + 强制新进程 + `--`
        wezterm_args_with(&exe_s, &full, pos, true, true),
        // #2 老版本不一定有 --always-new-process
        wezterm_args_with(&exe_s, &full, pos, false, true),
        // #3 没有 --position 的版本
        wezterm_args_with(&exe_s, &full, None, false, true),
        // #4 不吃 `--` 的版本
        wezterm_args_with(&exe_s, &full, None, false, false),
        // #5/#6 只关边框和标签栏，不传尺寸
        wezterm_args_with(&exe_s, &only_dec, None, false, true),
        wezterm_args_with(&exe_s, &only_dec, None, false, false),
        // #7 值不带引号
        wezterm_args_with(&exe_s, &bare_dec, None, false, true),
        // #8 旧写法：--config 放在 start 之后（大多数版本会报 unexpected argument）
        {
            let mut v = vec!["start".to_string()];
            v.extend_from_slice(&full);
            v.push("--".to_string());
            v.push(exe_s.clone());
            v
        },
    ]
}

fn push_cfg(v: &mut Vec<String>, key: &str, value: &str) {
    v.push("--config".to_string());
    v.push(format!("{key}={value}"));
}

/// 窗口外观：无标题栏 + 无标签栏 + 按网格尺寸开窗。
///
/// `enable_tab_bar=false` 是必须的：它不归 `window_decorations` 管，
/// 不关的话无标题栏窗口顶部还是会挂着一条标签栏（以及右侧滚动条）。
fn wezterm_opts(cols: u16, rows: u16, dec: &str) -> Vec<String> {
    let mut v = Vec::new();
    push_cfg(&mut v, "window_decorations", &format!("\"{dec}\""));
    push_cfg(&mut v, "enable_tab_bar", "false");
    push_cfg(&mut v, "initial_cols", &cols.to_string());
    push_cfg(&mut v, "initial_rows", &rows.to_string());
    // 格子之外那一圈不是我们能画到的地方：内边距、宽 / 高取整剩下的 1~2 px，
    // 都是用**终端自己的背景色**画的。用户主题是深色时，亮色界面四边就会出现
    // 一道黑边。所以内边距归零、滚动条关掉，并把终端背景改成界面底色。
    push_cfg(&mut v, "window_padding", "{left=0,right=0,top=0,bottom=0}");
    push_cfg(&mut v, "enable_scroll_bar", "false");
    if let Some(bg) = crate::theme::bg_hex() {
        push_cfg(&mut v, "colors", &format!("{{background=\"{bg}\"}}"));
    }
    v
}

/// 保守版本：只关边框和标签栏，不指定尺寸。
fn wezterm_plain_opts(dec: &str, quoted: bool) -> Vec<String> {
    let mut v = Vec::new();
    let dec_value = if quoted {
        format!("\"{dec}\"")
    } else {
        dec.to_string()
    };
    push_cfg(&mut v, "window_decorations", &dec_value);
    push_cfg(&mut v, "enable_tab_bar", "false");
    v
}

/// WezTerm 启动参数（推荐写法）。
///
/// 关键：`--config` 是**全局参数，必须写在子命令 `start` 之前**；
/// 写在后面会被 clap 拒绝（`error: unexpected argument '--config' found`）。
///
/// ```text
/// wezterm --config window_decorations="NONE" --config enable_tab_bar=false \
///         --config initial_cols=41 --config initial_rows=13 \
///         start --position X,Y --always-new-process -- <程序>
/// ```
///
/// `--config` 的值按 TOML 解析（带点号 / 内联表都行），所以字符串要带引号。
/// 具体组合见 [`wezterm_arg_variants`] 的第 1 种。
///
/// 拼参数：`[--config ...] start [--position x,y] [--always-new-process] [--] <exe>`
fn wezterm_args_with(
    exe: &str,
    opts: &[String],
    pos: Option<(i32, i32)>,
    new_process: bool,
    separator: bool,
) -> Vec<String> {
    let mut v = opts.to_vec();
    v.push("start".to_string());
    if let Some((x, y)) = pos {
        v.push("--position".to_string());
        v.push(format!("{x},{y}"));
    }
    if new_process {
        v.push("--always-new-process".to_string());
    }
    if separator {
        v.push("--".to_string());
    }
    v.push(exe.to_string());
    v
}

/// 尝试在支持无边框的终端（WezTerm → Alacritty）里重新启动自己。
pub fn borderless_relaunch(count: usize) -> bool {
    if std::env::var_os("APPDRAWER_IN_BORDERLESS").is_some() {
        crate::log::log("borderless: 跳过（APPDRAWER_IN_BORDERLESS 已设置，避免无限重启）");
        return false;
    }

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            crate::log::log(&format!("borderless: 无法获取当前程序路径: {e}"));
            return false;
        }
    };
    let (cols, rows) = compact_window_size(count);
    let pos = window_position(cols, rows);
    let cwd = std::env::current_dir().ok();
    crate::log::log(&format!(
        "borderless: 尝试开 {cols}x{rows} 无边框窗口，exe={exe:?}"
    ));

    // 1) WezTerm：一条命令搞定，跨平台（Windows / macOS / Linux）。
    let wezterm = if cfg!(windows) { "wezterm.exe" } else { "wezterm" };
    let candidates = borderless_candidates(wezterm);
    crate::log::log(&format!(
        "borderless: wezterm 候选 {} 个，前几个：{:?}",
        candidates.len(),
        &candidates[..candidates.len().min(3)]
    ));
    let variants = wezterm_arg_variants(&exe, cols, rows, decorations_mode(), pos);
    'wezterm: for prog in &candidates {
        for (i, args) in variants.iter().enumerate() {
            match spawn_quietly(prog, args, cwd.as_ref()) {
                SpawnOutcome::Running(child) => {
                    crate::log::log(&format!(
                        "borderless: 已用 {prog:?} 启动（pid={}，写法 #{}，decorations={}）",
                        child.id(),
                        i + 1,
                        decorations_mode()
                    ));
                    return true;
                }
                SpawnOutcome::Died(why) => {
                    crate::log::log(&format!(
                        "borderless: {prog:?} 写法 #{} 启动后退出：{why}",
                        i + 1
                    ));
                }
                // 这个路径里压根没这个程序，换下一个候选
                SpawnOutcome::Missing => continue 'wezterm,
            }
        }
        crate::log::log(&format!("borderless: {prog:?} 所有写法都被拒绝，放弃 wezterm"));
        break;
    }

    // 2) Alacritty：配置写临时文件，命令行只给 --config-file。
    let alacritty = if cfg!(windows) {
        "alacritty.exe"
    } else {
        "alacritty"
    };
    let candidates = borderless_candidates(alacritty);
    let cfg = alacritty_config(&exe, cols, rows, pos);
    let Ok(cfg_path) = write_script("toml", &cfg, false) else {
        crate::log::log("borderless: 写临时配置失败");
        return false;
    };
    let args = vec![
        "--config-file".to_string(),
        cfg_path.to_string_lossy().into_owned(),
    ];
    for prog in &candidates {
        match spawn_quietly(prog, &args, cwd.as_ref()) {
            SpawnOutcome::Running(child) => {
                crate::log::log(&format!(
                    "borderless: 已用 {prog:?} 启动（pid={}，config={cfg_path:?}）",
                    child.id()
                ));
                return true;
            }
            SpawnOutcome::Died(why) => {
                crate::log::log(&format!("borderless: {prog:?} 启动后退出：{why}"));
                break;
            }
            SpawnOutcome::Missing => {}
        }
    }

    crate::log::log("borderless: 没有可用的无边框终端，降级为带边框的窗口");
    false
}
/// 生成可执行文件的候选路径（纯函数，便于测试）。
///
/// 不靠 `is_file()` 判断：WindowsApps 里的 `wt.exe` 多为 App Execution Alias，
/// 文件系统元数据不可靠；只要 PATH 里出现过就交给系统解析，最后再兜底裸名字。
#[cfg(any(windows, test))]
pub fn program_candidates(file_name: &str, path_env: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            v.push(dir.join(file_name));
        }
    }
    v.push(PathBuf::from(file_name));
    v
}

/// `wt.exe` 的候选路径（`program_candidates` 的特例）。
#[cfg(any(windows, test))]
pub fn wt_candidates(path_env: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    program_candidates("wt.exe", path_env)
}

/// 把当前控制台的输入 / 输出代码页切到 UTF-8，避免中文与 emoji 乱码。
/// 非 Windows 平台为空操作。
pub fn enable_utf8_console() {
    #[cfg(windows)]
    unsafe {
        const CP_UTF8: u32 = 65001;
        win::SetConsoleOutputCP(CP_UTF8);
        win::SetConsoleCP(CP_UTF8);
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn SetConsoleOutputCP(code_page: u32) -> i32;
        pub fn SetConsoleCP(code_page: u32) -> i32;
        pub fn GetConsoleProcessList(pids: *mut u32, count: u32) -> u32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetConsoleWindow() -> *mut c_void;
        pub fn ShowWindow(hwnd: *mut c_void, cmd: i32) -> i32;
    }

    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOW: i32 = 5;
}

/// 双击启动时那个一闪而过的“cmd 窗口”是什么。
///
/// `appdrawer.exe` 是**控制台子系统**程序，所以 Windows 在我们的代码开始执行
/// **之前**就已经把控制台窗口建好并显示出来了（Windows 11 默认终端时还会先拉起
/// 整个 Windows Terminal），而这个窗口随后会被新开的无边框窗口取代——于是就有
/// 一次闪烁。
///
/// 这里在 `main` 的第一行把它藏起来，尽量缩短闪烁时间。
///
/// 判断依据是 `GetConsoleProcessList`：
/// * 只有本进程 → 这个控制台是为我们新建的（双击 / 从 Explorer 启动），可以藏；
/// * 还有别的进程（cmd.exe、PowerShell、bash……）→ 那是**用户的**终端窗口，
///   绝对不能藏，否则会把用户正在用的窗口弄没。
#[cfg(windows)]
static CONSOLE_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn hide_startup_console() {
    #[cfg(windows)]
    unsafe {
        let mut pids = [0u32; 16];
        let n = win::GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        let hwnd = win::GetConsoleWindow();
        if n <= 1 && !hwnd.is_null() {
            win::ShowWindow(hwnd, win::SW_HIDE);
            CONSOLE_HIDDEN.store(true, std::sync::atomic::Ordering::Relaxed);
            crate::log::log("console: 控制台只有本进程（双击启动），已隐藏初始控制台窗口");
        } else {
            crate::log::log(&format!(
                "console: 控制台里有 {n} 个进程（继承自终端），窗口不动"
            ));
        }
    }
}

/// 如果换窗口全部失败、要就地运行，就把刚藏起来的窗口还回来——
/// 否则用户会面对一个“程序在跑但看不见”的幽灵窗口。
pub fn restore_console_if_hidden() {
    #[cfg(windows)]
    if CONSOLE_HIDDEN.swap(false, std::sync::atomic::Ordering::Relaxed) {
        unsafe {
            let hwnd = win::GetConsoleWindow();
            if !hwnd.is_null() {
                win::ShowWindow(hwnd, win::SW_SHOW);
            }
        }
        crate::log::log("console: 没能换到新窗口，已把控制台窗口还回来");
    }
}

/// 是否运行在无法渲染 emoji 的旧版 Windows 控制台（conhost）。
/// 其它平台恒为 `false`。
pub fn legacy_console() -> bool {
    #[cfg(windows)]
    return windows_legacy_console();
    #[cfg(not(windows))]
    return false;
}

#[cfg(windows)]
fn windows_legacy_console() -> bool {
    // Windows Terminal 自身就能渲染 emoji。
    if std::env::var_os("WT_SESSION").is_some() {
        crate::log::log("legacy_console: false（WT_SESSION）");
        return false;
    }
    // 其它已知支持 emoji 的终端。
    for key in [
        "ConEmuANSI",
        "ConEmuPID",
        "WEZTERM_EXECUTABLE",
        "ALACRITTY_LOG",
        "VSCODE_INJECTION",
    ] {
        if std::env::var_os(key).is_some() {
            crate::log::log(&format!("legacy_console: false（现代终端 {key}）"));
            return false;
        }
    }
    crate::log::log("legacy_console: true（旧版控制台，将使用 ASCII 图标）");
    true
}

#[cfg(target_os = "macos")]
fn macos_new_window(name: &str, command: &str) -> io::Result<()> {
    let script = write_script("sh", &build_unix_script(name, command), true)?;
    Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(&script)
        .spawn()?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unix_new_terminal(name: &str, command: &str) -> io::Result<&'static str> {
    let script = write_script("sh", &build_unix_script(name, command), true)?;

    // (程序名, 前置参数)。未安装的程序 spawn 会失败，直接尝试下一个。
    const CANDIDATES: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("kgx", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("mate-terminal", &["--"]),
        ("tilix", &["-e"]),
        ("terminator", &["-x"]),
        ("kitty", &[]),
        ("alacritty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("foot", &[]),
        ("urxvt", &["-e"]),
        ("xterm", &["-e"]),
        ("st", &["-e"]),
    ];

    for (prog, prefix) in CANDIDATES {
        let mut cmd = Command::new(prog);
        cmd.args(*prefix);
        cmd.arg(&script);
        if cmd.spawn().is_ok() {
            return Ok(*prog);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "未找到可用的终端模拟器",
    ))
}

// ---------------------------------------------------------------------------
// 后台分离运行
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn spawn_detached(command: &str) -> io::Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(windows)]
fn spawn_detached(command: &str) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    Command::new("cmd")
        .arg("/C")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(unix, windows)))]
fn spawn_detached(_command: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Other,
        "当前平台不支持后台运行",
    ))
}

// ---------------------------------------------------------------------------
// 清理
// ---------------------------------------------------------------------------

/// 清理超过一天的旧会话脚本。
pub fn cleanup_old_scripts() {
    let dir = std::env::temp_dir().join("appdrawer-sessions");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .is_some_and(|m| m < cutoff);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_hiding_is_a_noop_off_windows() {
        // 非 Windows 上没有控制台窗口可藏；只要不 panic、不影响退出流程即可。
        hide_startup_console();
        restore_console_if_hidden();
        // restore 可以反复调（藏过才动作，还回去后就清标记）
        restore_console_if_hidden();
    }

    #[test]
    fn termux_script_has_shebang_command_and_stays_open() {
        let s = build_session_script("Test", "echo hello");
        assert!(s.starts_with(&format!("#!{}\n", TERMUX_BASH)));
        assert!(s.contains("\necho hello\n"));
        assert!(s.contains(&format!("exec {} -i", TERMUX_BASH)));
    }

    #[test]
    fn unix_script_uses_env_bash_and_keeps_window() {
        let s = build_unix_script("Test", "echo hello");
        assert!(s.starts_with("#!/usr/bin/env bash\n"));
        assert!(s.contains("\necho hello\n"));
        assert!(s.contains("exec \"${SHELL:-bash}\" -i"));
    }

    #[test]
    fn scripts_sanitize_name() {
        let s = build_session_script("a'b\nc", "true");
        assert!(!s.contains("'a'b"));
        assert!(s.contains("'AppDrawer: a b c'"));
    }

    #[test]
    fn scripts_survive_percent_and_comma_in_inputs() {
        let s = build_unix_script("50%", "echo a,b; printf '%s\\n' x");
        assert!(s.contains("echo a,b; printf '%s\\n' x"));
        assert!(s.contains("'AppDrawer: 50%'"));
    }

    #[test]
    fn windows_script_is_utf8_bom_powershell() {
        let s = build_windows_script("Get-Process");
        // UTF-8 BOM，保证 PowerShell 5.1 也能正确读取中文
        assert!(s.starts_with('\u{FEFF}'));
        assert!(s.contains("$ErrorActionPreference"));
        assert!(s.contains("Set-Location -LiteralPath $HOME"));
        // cmdlet 交给 PowerShell 执行，不用 Start-Process
        assert!(s.contains("\r\nGet-Process\r\n"));
        assert!(!s.contains("Start-Process"));
    }

    #[test]
    fn windows_simple_command_uses_start_process() {
        let s = build_windows_script("notepad");
        assert!(s.contains("Start-Process -FilePath 'notepad'"));

        let s = build_windows_script("git status --short");
        assert!(s.contains("Start-Process -FilePath 'git' -ArgumentList 'status --short'"));

        let s = build_windows_script(r#""C:\Program Files\App\app.exe" -x"#);
        assert!(s.contains(
            r#"Start-Process -FilePath 'C:\Program Files\App\app.exe' -ArgumentList '-x'"#
        ));
    }

    #[test]
    fn is_simple_command_distinguishes_powershell_builtins() {
        assert!(is_simple_command("notepad"));
        assert!(is_simple_command("explorer ."));
        assert!(is_simple_command(r#""C:\Program Files\App\app.exe" -flag"#));
        assert!(!is_simple_command("Get-Process"));
        assert!(!is_simple_command("Get-ChildItem -Force"));
        assert!(!is_simple_command("ls"));
        assert!(!is_simple_command("echo hi"));
        assert!(!is_simple_command("Get-Process | Sort-Object CPU"));
        assert!(!is_simple_command("$x = 1; Write-Output $x"));
        assert!(!is_simple_command(""));
    }

    #[test]
    fn wt_candidates_covers_path_and_fallback() {
        let paths = std::env::join_paths(["/a b", "/c"]).unwrap();
        let c = wt_candidates(Some(&paths));
        assert!(c.contains(&PathBuf::from("/a b").join("wt.exe")));
        assert!(c.contains(&PathBuf::from("/c").join("wt.exe")));
        // 最后一律兜底裸名字，交给系统按 PATH 解析。
        assert_eq!(c.last(), Some(&PathBuf::from("wt.exe")));
        // 没有 PATH 时也要有兜底候选。
        assert_eq!(wt_candidates(None), vec![PathBuf::from("wt.exe")]);
        // 空的 PATH 项被跳过。
        let empty = std::env::join_paths([""]).unwrap();
        assert_eq!(wt_candidates(Some(&empty)), vec![PathBuf::from("wt.exe")]);
    }

    #[test]
    fn wezterm_variants_degrade_gracefully() {
        let exe = std::path::Path::new("C:\\Apps\\appdrawer.exe");
        let pos = Some((763, 395));
        let v = wezterm_arg_variants(exe, 41, 13, "NONE", pos);
        assert_eq!(v.len(), 8);
        // 前两种带位置（居中）；后面没有 --position 的版本也能降级成功
        for args in &v[0..2] {
            assert!(args.iter().any(|a| a == "--position"));
            assert!(args.iter().any(|a| a == "763,395"));
        }
        for args in &v[2..] {
            assert!(!args.iter().any(|a| a == "--position"));
        }
        // #1 带 --always-new-process；#2 不带；#3 居中 + `--`；#4 连 `--` 也不带
        assert!(v[0].iter().any(|a| a == "--always-new-process"));
        assert!(!v[1].iter().any(|a| a == "--always-new-process"));
        assert!(v[1].iter().any(|a| a == "--"));
        assert!(v[2].iter().any(|a| a == "--"));
        assert!(v[2].iter().any(|a| a == "initial_cols=41"));
        assert!(!v[3].iter().any(|a| a == "--"));
        // #5/#6 只保留“无边框”和“无标签栏”
        for args in &v[4..6] {
            assert!(args.iter().any(|a| a == "window_decorations=\"NONE\""));
            assert!(!args.iter().any(|a| a.starts_with("initial_cols")));
        }
        // #7 值不带引号
        assert!(v[6].iter().any(|a| a == "window_decorations=NONE"));
        // 所有写法都以要运行的程序结尾，且都关掉了标签栏
        for args in &v {
            assert_eq!(args.last().unwrap(), "C:\\Apps\\appdrawer.exe");
            assert!(
                args.iter().any(|a| a == "enable_tab_bar=false"),
                "缺 enable_tab_bar=false: {args:?}"
            );
        }
        // 不给位置时就不带 --position
        let no_pos = wezterm_arg_variants(exe, 41, 13, "NONE", None);
        assert!(!no_pos[0].iter().any(|a| a == "--position"));
    }

    #[test]
    fn window_is_centered_in_the_work_area() {
        // 1920x1040 工作区，41x13 窗口，96 DPI（字号 12pt = 16px）
        // 单元 9.6 x 19.2 → 窗口 394 x 250 → 居中到 (763, 395)
        assert_eq!(centered_in(0, 0, 1920, 1040, 41, 13, 16.0), (763, 395));
        // 工作区有偏移（副屏 / 任务栏在左）时也要加上原点
        assert_eq!(centered_in(1920, 0, 1920, 1040, 41, 13, 16.0), (2683, 395));
        // 150% 缩放：字号 24px → 单元 14.4 x 28.8 → 窗口 590 x 374
        assert_eq!(centered_in(0, 0, 2560, 1400, 41, 13, 24.0), (985, 513));
        // 窗口比屏幕还大：不返回负坐标，贴左上角
        assert_eq!(centered_in(0, 0, 300, 200, 41, 13, 16.0), (0, 0));
    }

    #[test]
    fn window_pos_env_is_parsed() {
        assert_eq!(parse_pos("100,200"), Some((100, 200)));
        assert_eq!(parse_pos("  -5 , 7 "), Some((-5, 7)));
        assert_eq!(parse_pos("100"), None);
        assert_eq!(parse_pos("a,b"), None);
        assert_eq!(parse_pos(""), None);
    }

    #[test]
    fn wezterm_args_remove_the_frame() {
        let exe = std::path::Path::new("C:\\Apps\\appdrawer.exe");
        let args = wezterm_arg_variants(exe, 41, 13, "NONE", None)[0].clone();
        // --config 是全局参数，必须在子命令 start 之前（否则 clap 报
        // unexpected argument '--config'）
        assert_eq!(args[0], "--config");
        let start = args.iter().position(|a| a == "start").expect("缺少 start");
        let last_config = args.iter().rposition(|a| a == "--config").unwrap();
        assert!(last_config < start, "--config 跑到 start 后面了: {args:?}");

        assert!(args.iter().any(|a| a == "window_decorations=\"NONE\""));
        assert!(args.iter().any(|a| a == "enable_tab_bar=false"));
        assert!(args.iter().any(|a| a == "initial_cols=41"));
        assert!(args.iter().any(|a| a == "initial_rows=13"));
        // 格子之外那一圈是终端画的：内边距归零、关滚动条
        assert!(args.iter().any(|a| a == "window_padding={left=0,right=0,top=0,bottom=0}"));
        assert!(args.iter().any(|a| a == "enable_scroll_bar=false"));
        // 亮色主题下把终端背景色也改成界面底色（深色主题不覆盖，跟随用户配置）
        match crate::theme::bg_hex() {
            Some(bg) => assert!(
                args.iter().any(|a| a == &format!("colors={{background=\"{bg}\"}}")),
                "缺 colors 背景覆盖: {args:?}"
            ),
            None => assert!(!args.iter().any(|a| a.starts_with("colors="))),
        }
        // `--` 后面才是要运行的程序，而且必须是最后一个参数
        let sep = args.iter().position(|a| a == "--").expect("缺少 -- 分隔符");
        assert_eq!(args.len(), sep + 2);
        assert_eq!(args.last().unwrap(), "C:\\Apps\\appdrawer.exe");
        // 每种 --config 都单独带一个值
        let expected = if crate::theme::is_light() { 7 } else { 6 };
        assert_eq!(args.iter().filter(|a| *a == "--config").count(), expected);
    }

    #[test]
    fn alacritty_config_is_borderless_and_escaped() {
        let raw = "C:\\Program Files\\App \"Drawer\"\\appdrawer.exe";
        let cfg = alacritty_config(std::path::Path::new(raw), 41, 13, Some((763, 395)));
        assert!(cfg.contains("decorations = \"none\""));
        assert!(cfg.contains("columns = 41"));
        assert!(cfg.contains("lines = 13"));
        assert!(cfg.contains("position = { x = 763, y = 395 }"));
        // 亮色主题下连终端背景一起改，避免内边距 / 取整像素留黑边
        match crate::theme::bg_hex() {
            Some(bg) => assert!(
                cfg.contains(&format!("[colors.primary]\nbackground = '{bg}'")),
                "缺背景色:\n{cfg}"
            ),
            None => assert!(!cfg.contains("[colors.primary]")),
        }
        // 拿不到屏幕信息时不写 position（交给系统）
        let plain = alacritty_config(std::path::Path::new(raw), 41, 13, None);
        assert!(!plain.contains("position"));
        // 反斜杠与引号必须转义，否则 TOML 直接解析失败
        assert!(
            cfg.contains(r#"C:\\Program Files\\App \"Drawer\"\\appdrawer.exe"#),
            "路径没有正确转义:\n{cfg}"
        );
        assert!(!cfg.contains("decorations = \"full\""));
    }

    #[test]
    fn program_candidates_lists_path_then_bare_name() {
        let paths = std::env::join_paths(["/opt/wez", "/usr/local/bin"]).unwrap();
        let c = program_candidates("wezterm", Some(&paths));
        assert_eq!(c[0], PathBuf::from("/opt/wez").join("wezterm"));
        assert_eq!(c[1], PathBuf::from("/usr/local/bin").join("wezterm"));
        // 兜底裸名字，交给系统按 PATH 解析
        assert_eq!(c.last(), Some(&PathBuf::from("wezterm")));
        assert_eq!(program_candidates("wezterm", None), vec![PathBuf::from("wezterm")]);
    }

    #[test]
    fn compact_window_is_phone_sized() {
        // 7 个应用：4 列 x 9 格 + 左右各 1 列留白 = 41 列；高度取下限 13 行
        let (w, h) = compact_window_size(7);
        assert_eq!((w, h), (41, 13));
        // 应用多了窗口跟着变高，但封顶，不会疯长
        assert!(compact_window_size(40).1 > h);
        assert_eq!(compact_window_size(10_000).1, 45);
        // 这个尺寸下界面正好排 4 列，弹窗也放得下
        let l = crate::ui::Layout::compute(w, h);
        assert_eq!(l.columns, 4);
        assert!(l.visible_rows() >= 3, "{}x{} 只放得下 {} 行", w, h, l.visible_rows());
    }
}
