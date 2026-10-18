//! 无边框窗口的鼠标拖动。
//!
//! 无边框（`window_decorations="NONE"`）的窗口没有标题条，系统自然不给拖拽；
//! 终端自己的拖动（WezTerm 的 `StartWindowDrag`）在无边框窗口上也不生效。
//! 所以这里自己搬：在窗口里按住左键拖动，我们用 `SetWindowPos` 跟着鼠标挪。
//!
//! 为什么用**普通左键拖动**而不是 Alt+左键：
//! WezTerm/Windows Terminal 会在把鼠标事件交给程序之前先匹配它们自己的
//! `mouse_bindings`。用户为了拖窗口往往就绑了 Alt+左键 → `StartWindowDrag`，
//! 这个事件根本不会传给我们；而那个绑定在无边框窗口上又是空转的。
//! 普通左键拖动没有任何默认绑定，一定会传进来。（Alt+左键也支持，只是不指望它。）
//!
//! 好处是完全不依赖终端配色 / 装饰，坏处是要知道**宿主窗口的 HWND**——
//! 我们是个控制台进程，`GetConsoleWindow()` 拿到的是 conhost 的隐藏窗口，
//! 不是真正在屏幕上的那个。定位办法（按优先级）：
//!
//! 1. 按标题找：启动时我们用 `SetTitle("AppDrawer")` 把窗口标题定死了，
//!    `FindWindowW(nullptr, "AppDrawer")` 就能拿到；
//! 2. 退而求其次：`GetForegroundWindow()`，但必须**不是我们自己的进程**，
//!    且窗口类名看起来像终端，才敢动——不然有可能去搬别人的窗口。
//!
//! 两条路都失败就安静地不启用，绝不乱动窗口。

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Mutex;



    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
        fn GetForegroundWindow() -> *mut c_void;
        fn GetClassNameW(hwnd: *mut c_void, buf: *mut u16, max: i32) -> i32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        fn GetCursorPos(p: *mut Point) -> i32;
        fn GetWindowRect(hwnd: *mut c_void, r: *mut Rect) -> i32;
        fn SetWindowPos(
            hwnd: *mut c_void,
            after: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
    }

    /// 缓存找到的宿主窗口（0 = 还没找 / 找不到）。
    static HOST: AtomicIsize = AtomicIsize::new(0);
    static WARNED: Mutex<bool> = Mutex::new(false);

    struct Drag {
        hwnd: isize,
        /// 按下时的窗口左上角。
        win: (i32, i32),
        /// 按下时的鼠标屏幕坐标。
        cursor: (i32, i32),
    }

    static DRAG: Mutex<Option<Drag>> = Mutex::new(None);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn class_name(hwnd: isize) -> String {
        let mut buf = [0u16; 256];
        let n = unsafe { GetClassNameW(hwnd as *mut c_void, buf.as_mut_ptr(), buf.len() as i32) };
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }

    fn usable(hwnd: isize) -> bool {
        if hwnd == 0 {
            return false;
        }
        let h = hwnd as *mut c_void;
        if unsafe { IsWindowVisible(h) } == 0 {
            return false;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        // 我们自己进程的窗口（conhost 隐藏窗口）不能动
        pid != 0 && pid != std::process::id()
    }

    fn looks_like_terminal(class: &str) -> bool {
        let c = class.to_ascii_lowercase();
        [
            "wezterm",
            "alacritty",
            "windowsterminal",
            "cascadia",
            "conhost",
            "mintty",
        ]
        .iter()
        .any(|k| c.contains(k))
    }

    /// 找到宿主终端窗口。找不到返回 0。
    fn host_window() -> isize {
        let cached = HOST.load(Ordering::Relaxed);
        if cached != 0 {
            return cached;
        }

        // 1) 按我们设定的标题找
        let title = wide("AppDrawer");
        let by_title = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) } as isize;
        if usable(by_title) {
            HOST.store(by_title, Ordering::Relaxed);
            crate::log::log(&format!(
                "winmove: 宿主窗口 = 标题 AppDrawer，class={:?}",
                class_name(by_title)
            ));
            return by_title;
        }

        // 2) 退一步：前台窗口，但必须不是自己、且像终端
        let fg = unsafe { GetForegroundWindow() } as isize;
        let class = class_name(fg);
        if usable(fg) && looks_like_terminal(&class) {
            HOST.store(fg, Ordering::Relaxed);
            crate::log::log(&format!("winmove: 宿主窗口 = 前台窗口，class={class:?}"));
            return fg;
        }

        let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
        if !*warned {
            *warned = true;
            crate::log::log(&format!(
                "winmove: 找不到宿主窗口（标题命中={by_title}，前台 class={class:?}），Alt+拖动不可用"
            ));
        }
        0
    }

    fn cursor() -> Option<(i32, i32)> {
        let mut p = Point::default();
        if unsafe { GetCursorPos(&mut p) } == 0 {
            return None;
        }
        Some((p.x, p.y))
    }

    pub fn begin() -> bool {
        let hwnd = host_window();
        if hwnd == 0 {
            return false;
        }
        let mut r = Rect::default();
        if unsafe { GetWindowRect(hwnd as *mut c_void, &mut r) } == 0 {
            return false;
        }
        let Some(cur) = cursor() else {
            return false;
        };
        *DRAG.lock().unwrap_or_else(|e| e.into_inner()) = Some(Drag {
            hwnd,
            win: (r.left, r.top),
            cursor: cur,
        });
        true
    }

    pub fn follow() -> bool {
        let mut guard = DRAG.lock().unwrap_or_else(|e| e.into_inner());
        let Some(d) = guard.as_mut() else {
            return false;
        };
        let Some(cur) = cursor() else {
            return true;
        };
        let (x, y) = (
            d.win.0 + (cur.0 - d.cursor.0),
            d.win.1 + (cur.1 - d.cursor.1),
        );
        unsafe {
            SetWindowPos(
                d.hwnd as *mut c_void,
                std::ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        true
    }

    pub fn end() {
        *DRAG.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn active() -> bool {
        DRAG.lock().map(|g| g.is_some()).unwrap_or(false)
    }
}

#[cfg(windows)]
pub use imp::{active, begin, end, follow};

/// 非 Windows：窗口位置由窗口管理器 / 终端自己管，这里什么都不做。
#[cfg(not(windows))]
pub fn active() -> bool {
    false
}
#[cfg(not(windows))]
pub fn begin() -> bool {
    false
}
#[cfg(not(windows))]
pub fn follow() -> bool {
    false
}
#[cfg(not(windows))]
pub fn end() {}
