//! 文本显示宽度工具（CJK / emoji 按 2 列计算）。
//!
//! 终端里的“一格”不一定对应“一个 char”，虚拟缓冲和 VDOM 布局都依赖这里的宽度估算。

/// 估算单个字符在终端中的显示宽度（CJK / emoji 记为 2）。
pub fn char_width(c: char) -> usize {
    let cp = c as u32;
    if cp == 0 {
        return 0;
    }
    // 组合附加符号、零宽连接符、变体选择符不占宽度
    if cp == 0x200D || (0xFE00..=0xFE0F).contains(&cp) || (0x0300..=0x036F).contains(&cp) {
        return 0;
    }
    let wide = (0x1100..=0x115F).contains(&cp)        // 谚文字母
        || (0x2E80..=0x303E).contains(&cp)            // CJK 部首、标点
        || (0x3041..=0x33FF).contains(&cp)            // 假名、CJK 符号
        || (0x3400..=0x4DBF).contains(&cp)            // CJK 扩展 A
        || (0x4E00..=0x9FFF).contains(&cp)            // CJK 统一表意
        || (0xA000..=0xA4CF).contains(&cp)            // 彝文
        || (0xAC00..=0xD7A3).contains(&cp)            // 谚文音节
        || (0xF900..=0xFAFF).contains(&cp)            // CJK 兼容
        || (0xFE30..=0xFE6F).contains(&cp)            // CJK 兼容形式
        || (0xFF00..=0xFF60).contains(&cp)            // 全角形式
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x1F000..=0x1FAFF).contains(&cp)          // emoji / 符号
        || (0x2600..=0x27BF).contains(&cp)            // 杂项符号（emoji 风格）
        || (0x2B00..=0x2BFF).contains(&cp); // 杂项符号与箭头（⭐⬛⭕ 等）
    if wide { 2 } else { 1 }
}

/// 判断字符宽度是否在所有终端上都一致。
///
/// 只有宽度确定的字符才能信任终端光标的位置：ASCII 和制表符 / 中日韩全角
/// 字符总是 1 或 2 列；箭头、emoji、杂项符号的宽度可能因终端的“东亚宽度”
/// 设置而不同（1 列或 2 列）。
pub fn width_is_certain(c: char) -> bool {
    let cp = c as u32;
    c.is_ascii()
        || (0x2500..=0x257F).contains(&cp) // 制表符 ─│┌╮
        || (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0x303E).contains(&cp)
        || (0x3041..=0x33FF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE6F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
}

/// 判断一个宽字符是否是“宽度不确定”的 emoji / 杂项符号。
///
/// 中文、全角字符在所有终端上都是 2 列，但 emoji（尤其是 ☀ ⚠ ❤ 这类没带
/// U+FE0F 的文本展现字符）在不同终端 / 字体下可能被画成 1 列。
pub fn emoji_like(c: char) -> bool {
    let cp = c as u32;
    (0x1F000..=0x1FAFF).contains(&cp)
        || (0x2600..=0x27BF).contains(&cp)
        || (0x2B00..=0x2BFF).contains(&cp)
}

/// 计算字符串的显示宽度。
pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// 按显示宽度截断字符串。
pub fn truncate_to_width(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = char_width(ch);
        if w + cw > max {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

/// 右侧补空格直到显示宽度达到 `width`。
pub fn pad_to_width(s: &str, width: usize) -> String {
    let w = str_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(str_width("abc"), 3);
        assert_eq!(str_width("文件"), 4);
        assert_eq!(str_width("🐚"), 2);
        assert_eq!(str_width("⭐"), 2);
        assert_eq!(str_width("\u{fe0f}"), 0);
    }

    #[test]
    fn truncate_and_pad() {
        assert_eq!(truncate_to_width("文件abc", 5), "文件a");
        assert_eq!(str_width(&pad_to_width("文件", 5)), 5);
    }
}
