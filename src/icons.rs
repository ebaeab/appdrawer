//! 内置常用表情图标，以及图标选择器的状态与导航。
//!
//! 这里挑选的都是显示宽度为 2 的单个 emoji（或加变体选择符），
//! 以便在网格中整齐对齐；`ICONS` 的宽度由单元测试保证。

/// 内置图标（按用途分组，扁平存放便于网格导航）。
pub const ICONS: &[&str] = &[
    // 表情 / 人物
    "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😊", "😇", "🙂", "😉", "😍", "🥰", "😘",
    "😜", "🤪", "🤗", "🤔", "🤖", "👾", "👻", "💀", "👤", "👥",
    // 自然 / 天气
    "🌱", "🌿", "🍀", "🌵", "🌴", "🌳", "🌲", "🌸", "🌹", "🌺", "🌻", "🌼", "🍁", "🍂", "🍃",
    "☀️", "🌤️", "⛅", "☁️", "🌧️", "⛈️", "❄️", "⛄", "🌈", "🌙", "⭐", "🌟", "✨", "⚡", "🔥",
    "💧", "🌊",
    // 设备 / 工具
    "💻", "🖥️", "🖱️", "🖨️", "💾", "💿", "📀", "🧮", "📱", "☎️", "📞", "📟", "📠", "🔋", "🔌",
    "💡", "🔦", "🕯️", "🛠️", "🔧", "🔨", "⚙️", "🧰", "🔩", "⛓️", "🧲", "🧪", "🧬", "🔬", "🔭",
    "📡", "🛰️", "🚀", "🛸", "🧭", "🧱",
    // 影音 / 娱乐
    "🎵", "🎶", "🎤", "🎧", "🎷", "🎸", "🎹", "🎺", "🥁", "📻", "📺", "📼", "📷", "📸", "📹",
    "🎥", "🎬", "🎨", "🖌️", "🖍️", "🎮", "🕹️", "🎯", "🎲", "🧩",
    // 文件 / 办公
    "✉️", "📧", "📨", "📩", "📤", "📥", "📦", "📫", "📬", "📮", "🔗", "📎", "🖇️", "📌", "📍",
    "📁", "📂", "🗂️", "🗃️", "🗄️", "📄", "📃", "📑", "📊", "📈", "📉", "📋", "📇", "📆", "📅",
    "🗓️", "📖", "📚", "✏️", "🖊️", "🗑️",
    // 标记 / 状态 / 其它
    "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "✅", "❌", "⚠️", "❓", "❗", "➕",
    "➖", "✖️", "➗", "🔄", "🔁", "🔀", "🔒", "🔓", "🔑", "🗝️", "🛡️", "🔔", "🔕", "💬", "💭",
    "🗨️", "📢", "🌐", "🏠", "🏢", "🌍", "🌎", "🌏", "🔍", "🔎", "💰", "💳", "🛒", "🧠",
];

/// 旧版控制台（conhost）里使用的纯 ASCII 图标。
///
/// 宽度都不超过 2，能放进网格与选择器，且任何终端都能显示。
/// 仅在检测到无法渲染 emoji 的终端时启用（见 `set_ascii_mode`）。
pub const ASCII_ICONS: &[&str] = &[
    ":)", ":D", ";)", ":P", ":(", ":|", ":o", ":/", "<3", "=]", ":3", ":>", ":[", ":*",
    ":^", "^^", "@@", "**", "##", "$$", "%%", "++", "--", "==", "~~", "||", "[]", "{}",
    "()", "<>", "//", "..", "><", "oo",
];

/// 常用 emoji → 语义相近的 ASCII 表示（只影响显示，不改配置）。
const ASCII_FALLBACK: &[(&str, &str)] = &[
    ("😀", ":D"),
    ("😃", ":D"),
    ("😄", ":D"),
    ("😁", ":D"),
    ("😆", ":D"),
    ("😅", ":)"),
    ("😂", ":'"),
    ("🤣", ":'"),
    ("😊", ":)"),
    ("😇", "O)"),
    ("🙂", ":)"),
    ("😉", ";)"),
    ("😍", "<3"),
    ("🥰", "<3"),
    ("😘", ":*"),
    ("😜", ":P"),
    ("🤪", ":P"),
    ("🤗", "(:"),
    ("🤔", ":/"),
    ("🤖", "[]"),
    ("👾", "><"),
    ("👻", "oo"),
    ("💀", "xx"),
    ("👤", "()"),
    ("👥", "(("),
    ("🌱", "\\/"),
    ("🌿", "\\/"),
    ("🍀", "\\/"),
    ("🌵", "|/"),
    ("🌸", "**"),
    ("🌹", "@}"),
    ("🌻", "**"),
    ("☀️", "o/"),
    ("🌙", "C)"),
    ("⭐", "**"),
    ("🌟", "**"),
    ("✨", "**"),
    ("⚡", "//"),
    ("🔥", "##"),
    ("💧", ".."),
    ("🌊", "~~"),
    ("💻", "[]"),
    ("🖥️", "[]"),
    ("💾", "[]"),
    ("📱", "[]"),
    ("☎️", "T)"),
    ("🔋", "[]"),
    ("💡", "!"),
    ("🔧", "T)"),
    ("🔨", "T)"),
    ("⚙️", "++"),
    ("🚀", "=>"),
    ("🎵", "~)"),
    ("🎶", "~~"),
    ("📷", "[o]"),
    ("🎮", "[+]"),
    ("📺", "[]"),
    ("📻", "T)"),
    ("✉️", "<)"),
    ("📧", "<)"),
    ("📦", "[#]"),
    ("📁", "[]"),
    ("📂", "[]"),
    ("🗂️", "[]"),
    ("📄", ".."),
    ("📃", ".."),
    ("📝", "--"),
    ("✏️", "--"),
    ("📊", "||"),
    ("📈", "//"),
    ("📉", "\\\\"),
    ("📚", "[]"),
    ("📖", "[]"),
    ("❤️", "<3"),
    ("🧡", "<3"),
    ("💛", "<3"),
    ("💚", "<3"),
    ("💙", "<3"),
    ("💜", "<3"),
    ("✅", "v)"),
    ("❌", "x)"),
    ("⚠️", "/!"),
    ("❓", "?"),
    ("❗", "!"),
    ("➕", "+"),
    ("➖", "-"),
    ("🔒", "##"),
    ("🔓", "##"),
    ("🔑", "*k"),
    ("🔔", "@)"),
    ("💬", "<)"),
    ("📢", "!o"),
    ("🌐", "@@"),
    ("🏠", "^_"),
    ("🔍", "()"),
    ("🔎", "()"),
    ("💰", "$)"),
    ("💳", "[]"),
    ("🛒", "[+]"),
    ("🧠", "o_o"),
];

/// 兜底字符表情：未收录的 emoji 按哈希稳定地挑一个（保证可显示）。
const ASCII_EMOTES: &[&str] = &[
    ":)", ":D", ";)", ":P", ":(", ":|", ":o", ":/", "<3", "=]", ":3", "^^", "@@", "**", "##",
    "++", "--", "==", "~~", "||", "[]", "{}", "()", "<>", "//", "..", "oo", "><",
];

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// 是否使用纯 ASCII 图标（旧版控制台）。
static ASCII_MODE: AtomicBool = AtomicBool::new(false);

/// 设置图标渲染模式。
pub fn set_ascii_mode(on: bool) {
    ASCII_MODE.store(on, Ordering::Relaxed);
}

/// 是否处于 ASCII 图标模式。
pub fn ascii_mode() -> bool {
    ASCII_MODE.load(Ordering::Relaxed)
}

/// 指定模式下可用的图标集合（纯函数，便于测试）。
pub fn icons_for(ascii: bool) -> &'static [&'static str] {
    if ascii { ASCII_ICONS } else { ICONS }
}

/// 当前模式下可用的图标集合。
pub fn active_icons() -> &'static [&'static str] {
    icons_for(ascii_mode())
}

/// ASCII 模式下，为某个（非 ASCII）图标挑选一个字符表情。
pub fn fallback_ascii(icon: &str) -> &'static str {
    if let Some((_, a)) = ASCII_FALLBACK.iter().find(|(e, _)| *e == icon) {
        return a;
    }
    // FNV-1a 哈希：同一图标每次都得到相同结果。
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in icon.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    ASCII_EMOTES[(h as usize) % ASCII_EMOTES.len()]
}

/// 返回实际用于渲染的图标文本（纯函数，便于测试）。
///
/// * 非 ASCII 模式：原样返回；
/// * ASCII 模式：`icon` 已是 ASCII 就原样使用，否则用字符表情替代。
pub fn render_icon_for(icon: &str, ascii: bool) -> Cow<'_, str> {
    if !ascii || icon.is_ascii() {
        Cow::Borrowed(icon)
    } else {
        Cow::Borrowed(fallback_ascii(icon))
    }
}

/// 按当前模式渲染图标。
pub fn render_icon(icon: &str) -> Cow<'_, str> {
    render_icon_for(icon, ascii_mode())
}

/// 图标选择器状态。
pub struct Picker {
    /// 当前选中的图标下标。
    pub selected: usize,
}

impl Default for Picker {
    fn default() -> Self {
        Self::new()
    }
}

impl Picker {
    pub fn new() -> Self {
        Picker { selected: 0 }
    }

    /// 被选中的图标。
    pub fn current(&self) -> &'static str {
        let icons = active_icons();
        icons[self.selected.min(icons.len().saturating_sub(1))]
    }

    pub fn left(&mut self, cols: usize) {
        if cols > 0 && !self.selected.is_multiple_of(cols) {
            self.selected -= 1;
        }
    }

    pub fn right(&mut self, cols: usize) {
        if cols > 0 && self.selected % cols + 1 < cols && self.selected + 1 < active_icons().len() {
            self.selected += 1;
        }
    }

    pub fn up(&mut self, cols: usize) {
        if self.selected >= cols {
            self.selected -= cols;
        }
    }

    pub fn down(&mut self, cols: usize) {
        if self.selected + cols < active_icons().len() {
            self.selected += cols;
        }
    }

    /// 翻页（`dir` 为 +1 / -1）。
    pub fn page(&mut self, cols: usize, rows: usize, dir: isize) {
        let step = (cols * rows).max(1) as isize;
        let target = self.selected as isize + dir * step;
        self.selected = target.clamp(0, active_icons().len().saturating_sub(1) as isize) as usize;
    }

    pub fn home(&mut self) {
        self.selected = 0;
    }

    pub fn end(&mut self) {
        self.selected = active_icons().len().saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::str_width;

    #[test]
    fn all_icons_are_display_width_two() {
        for icon in ICONS {
            assert_eq!(
                str_width(icon),
                2,
                "图标 {icon:?} 的显示宽度不是 2，会导致网格错位"
            );
        }
    }

    #[test]
    fn picker_navigation_stays_in_bounds() {
        let cols = 8;
        let mut p = Picker::new();
        p.left(cols);
        assert_eq!(p.selected, 0);
        p.up(cols);
        assert_eq!(p.selected, 0);
        p.right(cols);
        assert_eq!(p.selected, 1);
        p.down(cols);
        assert_eq!(p.selected, 9);
        p.end();
        assert_eq!(p.selected, ICONS.len() - 1);
        p.right(cols); // 行末不能越到下一行
        assert_eq!(p.selected, ICONS.len() - 1);
        p.home();
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn picker_page_clamps() {
        let mut p = Picker::new();
        p.page(8, 5, 1);
        assert_eq!(p.selected, 40);
        p.page(8, 5, -1);
        assert_eq!(p.selected, 0);
        p.page(8, 5, -1);
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn ascii_icons_are_ascii_and_narrow() {
        for icon in ASCII_ICONS {
            assert!(icon.is_ascii(), "ASCII 图标 {icon:?} 含非 ASCII 字符");
            assert!(
                str_width(icon) <= 2,
                "ASCII 图标 {icon:?} 过宽（{}）",
                str_width(icon)
            );
        }
    }

    #[test]
    fn ascii_fallback_is_deterministic() {
        let a = fallback_ascii("🐚");
        let b = fallback_ascii("🐚");
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(a.is_ascii());
    }

    #[test]
    fn render_icon_for_only_substitutes_in_ascii_mode() {
        // 非 ASCII 模式原样返回。
        assert_eq!(render_icon_for("🐚", false).as_ref(), "🐚");
        // ASCII 模式替换为字符表情。
        assert!(render_icon_for("🐚", true).is_ascii());
        // 本来就是 ASCII 的图标保持原样。
        assert_eq!(render_icon_for(":)", true).as_ref(), ":)");
        // 图标集合跟随模式切换。
        assert_eq!(icons_for(true), ASCII_ICONS);
        assert_eq!(icons_for(false), ICONS);
    }
}

