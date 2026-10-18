//! 配色：亮色 / 深色两套。
//!
//! 由 `APPDRAWER_THEME=light|dark` 选择，**默认亮色**。
//!
//! 为什么不用“跟随终端主题”（到处 `Color::Reset`）：那样界面好不好看完全取决于
//! 用户在终端里配的配色，标题栏、选中格、弹窗背景都要靠猜。这里所有颜色都显式给出，
//! 网格区也自己铺底色，所以 AppDrawer 窗口在任何终端里长得都一样。

use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::style::Color;

/// 默认亮色（用户要的“窗口背景亮色”）。
static LIGHT: AtomicBool = AtomicBool::new(true);

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

/// 解析 `APPDRAWER_THEME`；不认识的取值按亮色处理。
pub fn parse_theme(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "dark" | "0" | "false" | "no" | "off" | "night"
    )
}

/// 从环境变量定下主题（main 里调用一次）。
pub fn init() {
    let raw = std::env::var("APPDRAWER_THEME").unwrap_or_default();
    let light = parse_theme(&raw);
    LIGHT.store(light, Ordering::Relaxed);
    if !raw.is_empty() && !matches!(raw.trim().to_ascii_lowercase().as_str(), "dark" | "light") {
        crate::log::log(&format!("theme: APPDRAWER_THEME={raw:?} 不认识，用亮色"));
    }
    crate::log::log(&format!("theme = {}", if light { "light" } else { "dark" }));
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

// ---- 基础色 ----------------------------------------------------------------

/// 整个窗口的底色（网格、空白区）。
///
/// 亮色用暖白 `#FFFEF8`（不是纯白）：配合青绿色托栏，整块看起来没那么刺眼。
pub fn bg() -> Color {
    if is_light() { rgb(255, 254, 248) } else { Color::Reset }
}

/// 窗口底色写入终端配置用的十六进制颜色。
///
/// 亮色主题下返回 `Some("#FFFEF8")`，深色主题返回 `None`（不覆盖，跟随用户自己的主题）。
/// 为什么要告诉终端：格子之外那一圈（内边距、宽 / 高取整剩下的 1~2 px）不是我们
/// 能画到的地方，它是用**终端自己的背景色**画的——用户主题是深色时，
/// 亮色界面周围就会出现一道“黑边”。
pub fn bg_hex() -> Option<&'static str> {
    if is_light() { Some("#FFFEF8") } else { None }
}

/// 正文前景色。
pub fn fg() -> Color {
    if is_light() { rgb(32, 34, 40) } else { Color::Reset }
}

/// 次要文字（未选中的应用名、提示文字）。
pub fn muted() -> Color {
    if is_light() { rgb(108, 112, 124) } else { Color::Grey }
}

/// 强调色（选中格、聚焦字段、弹窗边框）。
pub fn accent() -> Color {
    if is_light() { rgb(0, 122, 204) } else { rgb(0, 120, 215) }
}

/// 强调色底上的文字。
pub fn on_accent() -> Color {
    Color::White
}

/// 顶栏 / 底栏底色：青绿 `#57C3C2`。
pub fn bar() -> Color {
    if is_light() { rgb(87, 195, 194) } else { rgb(0, 90, 158) }
}

/// 顶栏 / 底栏文字。
///
/// 亮色下不能用白字：`#57C3C2` 的相对亮度是 0.45，白字对比度只有 2.1:1，
/// 远低于 4.5:1 的可读下限；换成墨绿 `#0F3A3A` 约 5.9:1。
pub fn on_bar() -> Color {
    if is_light() { rgb(15, 58, 58) } else { Color::White }
}

/// 命令预览等“提示”文字。
pub fn hint() -> Color {
    if is_light() { rgb(0, 110, 140) } else { Color::DarkCyan }
}

/// 状态提示（偏警告）。
pub fn warn() -> Color {
    if is_light() { rgb(160, 90, 0) } else { Color::Yellow }
}

/// 错误 / 删除确认。
pub fn danger() -> Color {
    if is_light() { rgb(190, 40, 40) } else { Color::Red }
}

// ---- 弹窗 ----------------------------------------------------------------

/// 弹窗底色。
pub fn modal_bg() -> Color {
    if is_light() { rgb(255, 255, 255) } else { rgb(24, 26, 34) }
}

/// 弹窗正文色（亮色主题下必须是深色，否则白底白字）。
pub fn modal_fg() -> Color {
    if is_light() { rgb(32, 34, 40) } else { Color::White }
}

/// 输入框底色。
pub fn field() -> Color {
    if is_light() { rgb(238, 240, 245) } else { rgb(40, 44, 58) }
}

/// 输入框文字。
pub fn field_fg() -> Color {
    if is_light() { rgb(32, 34, 40) } else { Color::White }
}

/// 聚焦输入框底色。
pub fn field_focus() -> Color {
    if is_light() { rgb(0, 122, 204) } else { rgb(0, 90, 170) }
}

/// 聚焦输入框文字。
pub fn on_field_focus() -> Color {
    Color::White
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bg_hex_matches_the_palette() {
        if is_light() {
            assert_eq!(bg_hex(), Some("#FFFEF8"));
        } else {
            assert_eq!(bg_hex(), None);
        }
    }

    /// 相对亮度（WCAG 2.x）。命名色只支持本模块用到的几个。
    fn luminance(c: Color) -> f64 {
        let (r, g, b) = match c {
            Color::Rgb { r, g, b } => (r, g, b),
            Color::White => (255, 255, 255),
            Color::Black => (0, 0, 0),
            other => panic!("测试里没给 {other:?} 定亮度"),
        };
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn text_is_readable_on_every_surface() {
        if !is_light() {
            return;
        }
        // 顶栏 / 底栏：深墨绿压在青绿上，别为了好看牺牲可读性
        assert!(
            contrast(on_bar(), bar()) >= 4.5,
            "栏上文字对比度只有 {:.2}:1",
            contrast(on_bar(), bar())
        );
        assert!(contrast(fg(), bg()) >= 7.0);
        assert!(contrast(modal_fg(), modal_bg()) >= 7.0);
        assert!(contrast(field_fg(), field()) >= 7.0);
        assert!(contrast(on_field_focus(), field_focus()) >= 4.5);
        assert!(
            contrast(on_accent(), accent()) >= 4.5,
            "选中格文字对比度只有 {:.2}:1",
            contrast(on_accent(), accent())
        );
    }

    #[test]
    fn theme_env_parsing() {
        assert!(parse_theme("light"));
        assert!(parse_theme(""));
        assert!(parse_theme("whatever"));
        assert!(!parse_theme("dark"));
        assert!(!parse_theme("DARK"));
        assert!(!parse_theme(" off "));
        assert!(!parse_theme("0"));
        assert!(!parse_theme("night"));
    }

    #[test]
    fn default_is_light_and_colors_are_visible() {
        // 默认（测试环境不设变量）就是亮色
        if std::env::var_os("APPDRAWER_THEME").is_none() {
            assert!(is_light());
            assert_ne!(bg(), Color::Reset);
            // 亮色主题下：白底不能用白字
            assert_ne!(modal_bg(), modal_fg());
            assert_ne!(field(), field_fg());
        }
    }
}
