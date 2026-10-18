//! 应用清单：内置默认应用，并支持通过配置文件自定义。
//!
//! 配置文件格式（每行一个应用）：
//! ```text
//! 图标|名称|命令
//! ```
//! 以 `#` 开头的行是注释，空行会被忽略。
//! 默认路径：Unix 下为 `$HOME/.config/appdrawer/apps.conf`，Windows 下为
//! `%APPDATA%\appdrawer\apps.conf`；可用环境变量 `APPDRAWER_CONFIG` 覆盖。

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

/// 菜单中的一个应用按钮。
#[derive(Clone, Debug)]
pub struct App {
    /// 图标（建议使用宽度为 2 的 emoji，例如 "🐚"）。
    pub icon: String,
    /// 显示名称。
    pub name: String,
    /// 点击后执行的命令（在新窗口 / 新会话中运行）。
    pub command: String,
}

/// 内置默认应用（按平台选择，缺失的命令可在配置文件中替换）。
pub fn defaults() -> Vec<App> {
    // Termux / Android
    #[cfg(target_os = "android")]
    const RAW: &[(&str, &str, &str)] = &[
        ("🐚", "Shell", "bash"),
        ("📁", "Files", "ls -la --color=auto"),
        ("📝", "Editor", "nano"),
        ("📊", "htop", "htop"),
        ("📈", "Top", "top"),
        ("🌐", "Net", "curl -s ifconfig.me; echo"),
        ("🔋", "Battery", "termux-battery-status"),
        ("📶", "WiFi", "termux-wifi-connectioninfo"),
        ("📷", "Camera", "termux-camera-photo -c 0 $HOME/photo.jpg"),
        ("🔔", "Toast", "termux-toast '来自 AppDrawer 的问候'"),
        ("🔊", "Volume", "termux-volume"),
        ("💻", "System", "fastfetch"),
    ];

    // Windows
    #[cfg(windows)]
    const RAW: &[(&str, &str, &str)] = &[
        ("🖥️", "PowerShell", "powershell"),
        ("📁", "Explorer", "explorer ."),
        ("📝", "Notepad", "notepad"),
        ("🌐", "Net", "ping 8.8.8.8 -t"),
        (
            "📊",
            "Processes",
            "Get-Process | Sort-Object CPU -Descending | Select-Object -First 20",
        ),
        ("📈", "System", "Get-ComputerInfo"),
        ("📅", "Date", "Get-Date"),
        ("🔊", "Volume", "sndvol"),
        ("🧮", "Calc", "calc"),
    ];

    // 其它 Unix（Linux / macOS 等）
    #[cfg(not(any(target_os = "android", windows)))]
    const RAW: &[(&str, &str, &str)] = &[
        ("🐚", "Shell", "bash"),
        ("📁", "Files", "ls -la --color=auto"),
        ("📝", "Editor", "nano"),
        ("📊", "htop", "htop"),
        ("📈", "Top", "top"),
        ("🌐", "Net", "curl -s ifconfig.me; echo"),
        ("🖥️", "System", "uname -a"),
        ("📅", "Date", "date"),
        ("💾", "Disk", "df -h"),
    ];

    RAW.iter()
        .map(|(icon, name, command)| App {
            icon: (*icon).to_string(),
            name: (*name).to_string(),
            command: (*command).to_string(),
        })
        .collect()
}

/// 返回配置文件的绝对路径。
pub fn config_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("APPDRAWER_CONFIG")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;

    #[cfg(windows)]
    if let Some(appdata) = env::var_os("APPDATA") {
        if !appdata.is_empty() {
            return Some(PathBuf::from(appdata).join("appdrawer").join("apps.conf"));
        }
    }

    Some(PathBuf::from(home).join(".config").join("appdrawer").join("apps.conf"))
}

/// 解析配置文本。
fn parse(text: &str) -> Vec<App> {
    let mut apps = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 {
            continue;
        }
        let icon = parts[0].trim();
        let name = parts[1].trim();
        let command = parts[2].trim();
        if name.is_empty() || command.is_empty() {
            continue;
        }
        apps.push(App {
            icon: if icon.is_empty() { "▫".to_string() } else { icon.to_string() },
            name: name.to_string(),
            command: command.to_string(),
        });
    }
    apps
}

/// 将应用清单序列化为配置文本。
fn serialize(apps: &[App]) -> String {
    let mut s = String::from("# AppDrawer 配置：每行格式为  图标|名称|命令\n# 以 # 开头的行为注释。\n# 也可在菜单内直接编辑：a 添加 / e 修改 / d 删除 / r 重新加载\n\n");
    for a in apps {
        s.push_str(&format!("{}|{}|{}\n", a.icon, a.name, a.command));
    }
    s
}

/// 加载应用清单：优先读取配置文件；若不存在则写入默认配置并返回默认值。
pub fn load() -> Vec<App> {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                let apps = parse(&text);
                if !apps.is_empty() {
                    return apps;
                }
            }
        } else {
            let defs = defaults();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, serialize(&defs));
            return defs;
        }
    }
    defaults()
}

/// 将当前应用清单写回配置文件（用于界面内的增删改）。
pub fn save(apps: &[App]) -> io::Result<()> {
    let path = config_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "无法确定配置文件路径（缺少 HOME）")
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serialize(apps))?;
    Ok(())
}
