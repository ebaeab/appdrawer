//! 极简启动日志：默认写到临时目录的 `appdrawer.log`，用于排查
//! “为什么没有在 Windows Terminal 里启动”之类的问题。
//!
//! 可用环境变量 `APPDRAWER_LOG` 指定日志文件路径。
//! 任何写入失败都会被静默忽略，绝不影响主程序。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 日志文件超过该大小就先清空，避免无限增长。
const MAX_BYTES: u64 = 512 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

/// 日志文件路径（`APPDRAWER_LOG` 可覆盖）。
pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("APPDRAWER_LOG") {
        return PathBuf::from(p);
    }
    std::env::temp_dir().join("appdrawer.log")
}

/// 追加一行带毫秒时间戳的日志。
pub fn log(msg: &str) {
    let _guard = LOCK.lock();
    let file = path();

    // 过大就清空重来。
    if let Ok(meta) = std::fs::metadata(&file)
        && meta.len() > MAX_BYTES
    {
        let _ = std::fs::remove_file(&file);
    }

    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) else {
        return;
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(f, "[{ts}] {msg}");
}
