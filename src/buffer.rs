//! 终端虚拟缓冲（virtual terminal buffer）。
//!
//! 思路参考 <https://github.com/tsoding/4at>：
//! 先把整屏内容画进内存里的 `Buffer`，再和上一帧 `diff`，
//! 只把真正变化的单元格通过转义序列写到终端。
//!
//! 这样界面代码可以“整屏重画”，而终端实际收到的写入量很小，不会闪烁。

use std::io::{self, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};

use crate::text::{char_width, emoji_like, width_is_certain};

/// 单元格样式。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            bold: false,
        }
    }
}

impl Style {
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self {
            fg,
            bg,
            bold: false,
        }
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

/// 屏幕上的一个单元格。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
    /// 宽字符（如 emoji / 中文）占用两格，右半格只做占位，不单独输出。
    pub cont: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
            cont: false,
        }
    }
}

/// 矩形区域（坐标单位为终端字符格）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> u16 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> u16 {
        self.y.saturating_add(self.h)
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// 一处需要写到终端的差异。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Patch {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

/// 内存中的整屏缓冲。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Buffer {
    pub width: usize,
    pub height: usize,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); width * height];
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        let (x, y) = (x as usize, y as usize);
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    pub fn cell(&self, x: u16, y: u16) -> Option<Cell> {
        self.index(x, y).map(|i| self.cells[i])
    }

    fn set_raw(&mut self, x: u16, y: u16, cell: Cell) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = cell;
        }
    }

    /// 写入一个字符，自动处理宽字符占用的两格。
    pub fn put_char(&mut self, x: u16, y: u16, ch: char, style: Style) {
        let w = char_width(ch);
        if w == 0 || self.index(x, y).is_none() {
            return;
        }
        // 宽字符必须有两格空间，否则宁可不画，避免折行。
        if w == 2 && self.index(x + 1, y).is_none() {
            return;
        }

        // 落点是某个宽字符的右半格：先把左半格改成空格。
        if self.cell(x, y).is_some_and(|c| c.cont) && x > 0 {
            let mut left = self.cell(x - 1, y).unwrap_or_default();
            left.ch = ' ';
            left.cont = false;
            self.set_raw(x - 1, y, left);
        }

        // 落点原本是宽字符的左半格：先清掉它右边的占位格。
        let stale_right = self
            .cell(x, y)
            .filter(|old| !old.cont && char_width(old.ch) == 2)
            .and_then(|_| self.cell(x + 1, y))
            .filter(|right| right.cont)
            .map(|right| right.style);
        if let Some(style) = stale_right {
            self.set_raw(
                x + 1,
                y,
                Cell {
                    ch: ' ',
                    style,
                    cont: false,
                },
            );
        }

        self.set_raw(
            x,
            y,
            Cell {
                ch,
                style,
                cont: false,
            },
        );

        if w == 2 {
            self.set_raw(
                x + 1,
                y,
                Cell {
                    ch: ' ',
                    style,
                    cont: true,
                },
            );
        } else if let Some(right) = self.cell(x + 1, y) {
            // 用窄字符覆盖了宽字符的左半格时，右边不能留下孤立的占位格。
            if right.cont {
                self.set_raw(
                    x + 1,
                    y,
                    Cell {
                        ch: ' ',
                        style: right.style,
                        cont: false,
                    },
                );
            }
        }
    }

    /// 从 `(x, y)` 开始写入字符串，返回结束后下一格的 x 坐标。
    pub fn put_str(&mut self, x: u16, y: u16, s: &str, style: Style) -> u16 {
        let mut cx = x;
        for ch in s.chars() {
            let w = char_width(ch);
            if w == 0 {
                continue;
            }
            if (cx as usize) + w > self.width {
                break;
            }
            self.put_char(cx, y, ch, style);
            cx = cx.saturating_add(w as u16);
        }
        cx
    }

    /// 用空格和给定样式填充矩形。
    pub fn fill(&mut self, rect: Rect, style: Style) {
        for dy in 0..rect.h {
            for dx in 0..rect.w {
                self.put_char(
                    rect.x.saturating_add(dx),
                    rect.y.saturating_add(dy),
                    ' ',
                    style,
                );
            }
        }
    }

    /// 与上一帧比较，返回需要重绘的单元格（跳过宽字符占位格）。
    pub fn diff(&self, old: &Buffer) -> Vec<Patch> {
        // 尺寸不同无法逐格比较，等价于和一块空白缓冲比较（调用方会先清屏）。
        if self.width != old.width || self.height != old.height {
            let blank = Buffer::new(self.width, self.height);
            return self.diff(&blank);
        }

        let mut patches = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let new = self.cells[i];
                // 宽字符的右半格从不单独输出，由左半格负责。
                if new.cont {
                    continue;
                }
                if new == old.cells[i] {
                    continue;
                }
                if char_width(new.ch) == 2 {
                    // 先把右半格的残留旧内容擦掉再画宽字符：即使某些终端把
                    // emoji 渲染成 1 列（如 Windows 控制台），也不会留下半个旧字符。
                    // emoji 一类“宽度不确定”的字符一律先擦；中文则只在右半格
                    // 确实有可见内容时才擦，避免给每个汉字都多发一个空格。
                    let right = i + 1;
                    if right < self.cells.len()
                        && !old.cells[right].cont
                        && (emoji_like(new.ch) || old.cells[right].ch != ' ')
                    {
                        patches.push(Patch {
                            x: (x + 1) as u16,
                            y: y as u16,
                            cell: Cell {
                                ch: ' ',
                                style: new.style,
                                cont: false,
                            },
                        });
                    }
                }
                patches.push(Patch {
                    x: x as u16,
                    y: y as u16,
                    cell: new,
                });
            }
        }
        patches
    }

    /// 把缓冲内容导出为文本（每行一个 `String`，宽字符只计一次）。
    #[cfg(test)]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            if y > 0 {
                out.push('\n');
            }
            for x in 0..self.width {
                let cell = self.cells[y * self.width + x];
                if cell.cont {
                    continue;
                }
                out.push(cell.ch);
            }
        }
        out
    }

    /// 拷贝另一块同尺寸缓冲的内容。
    pub fn copy_from(&mut self, other: &Buffer) {
        if self.width == other.width && self.height == other.height {
            self.cells.copy_from_slice(&other.cells);
        } else {
            self.width = other.width;
            self.height = other.height;
            self.cells.clone_from(&other.cells);
        }
    }
}

/// 把差异写成转义序列。相邻单元格会复用光标位置，颜色也会缓存。
pub fn apply_patches<W: Write>(out: &mut W, patches: &[Patch]) -> io::Result<()> {
    if patches.is_empty() {
        return Ok(());
    }
    // 先重置所有属性（SGR 0），确保不会残留下划线 / 反色等状态。
    // 注意：不要用 `Attribute::NoBold`，它的 SGR 码是 21，
    // 在 ECMA-48 / Windows Terminal 里表示“双下划线”（而不是取消加粗），
    // 会在文字下面留下双横线。取消加粗要用 `NormalIntensity`（SGR 22）。
    let mut style = Style::default();
    queue!(
        out,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(style.fg),
        SetBackgroundColor(style.bg)
    )?;

    // `cursor_x` 是终端当前光标所在的列（考虑宽字符后移 2 格）。
    let mut cursor: Option<(u16, u16)> = None;

    for p in patches {
        let contiguous = matches!(cursor, Some((x, y)) if y == p.y && x == p.x);
        if !contiguous {
            queue!(out, MoveTo(p.x, p.y))?;
        }

        if p.cell.style != style {
            if p.cell.style.fg != style.fg {
                queue!(out, SetForegroundColor(p.cell.style.fg))?;
            }
            if p.cell.style.bg != style.bg {
                queue!(out, SetBackgroundColor(p.cell.style.bg))?;
            }
            if p.cell.style.bold != style.bold {
                queue!(
                    out,
                    SetAttribute(if p.cell.style.bold {
                        Attribute::Bold
                    } else {
                        Attribute::NormalIntensity
                    })
                )?;
            }
            style = p.cell.style;
        }

        queue!(out, Print(p.cell.ch))?;
        // 只有宽度确定的字符才能信任光标位置。箭头、emoji 等字符的实际占位
        // 可能和 `char_width` 不一致（旧版 Windows 控制台或某些字体把 emoji
        // 画成 1 列），如果接着用光标推算下一格，整行就会逐格向左漂移，
        // 弹窗右边框看起来就“没闭合”。所以它们之后一律重新定位。
        cursor = if width_is_certain(p.cell.ch) {
            Some((p.x.saturating_add(char_width(p.cell.ch) as u16), p.y))
        } else {
            None
        };
    }
    Ok(())
}

/// 双缓冲渲染器：`curr` 是当前帧，`prev` 是上一帧。
///
/// 用法：
/// ```ignore
/// renderer.buffer_mut().clear();      // 或由 VDOM 负责清屏并重画
/// draw_into(renderer.buffer_mut());
/// renderer.present(&mut stdout)?;     // 只写差异
/// stdout.flush()?;
/// ```
pub struct Renderer {
    prev: Buffer,
    curr: Buffer,
    full: bool,
}

impl Renderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            prev: Buffer::new(width, height),
            curr: Buffer::new(width, height),
            full: true,
        }
    }

    /// 终端尺寸变化：清空两个缓冲，下一帧强制全量重绘。
    pub fn resize(&mut self, width: usize, height: usize) {
        self.prev.resize(width, height);
        self.curr.resize(width, height);
        self.full = true;
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.curr
    }

    /// 把 `curr` 相对 `prev` 的差异写入 `out`。成功返回差异数量。
    ///
    /// 注意：本方法不调用 `flush`，方便调用方在写光标位置后统一 flush。
    pub fn present<W: Write>(&mut self, out: &mut W) -> io::Result<usize> {
        let count;
        if self.full {
            // 终端尺寸可能变了，旧内容不可信：先清屏，再全量重画非默认单元格。
            self.full = false;
            queue!(
                out,
                SetAttribute(Attribute::Reset),
                SetForegroundColor(Color::Reset),
                SetBackgroundColor(Color::Reset),
                Clear(ClearType::All)
            )?;
            let blank = Buffer::new(self.curr.width, self.curr.height);
            let patches = self.curr.diff(&blank);
            count = patches.len();
            apply_patches(out, &patches)?;
        } else {
            let patches = self.curr.diff(&self.prev);
            count = patches.len();
            apply_patches(out, &patches)?;
        }
        self.prev.copy_from(&self.curr);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_reports_only_changed_cells() {
        let mut a = Buffer::new(10, 3);
        let mut b = Buffer::new(10, 3);
        b.put_char(2, 1, 'X', Style::default());
        assert_eq!(a.diff(&b).len(), 1);
        assert_eq!(b.diff(&a).len(), 1);

        a.put_char(2, 1, 'X', Style::default());
        assert!(a.diff(&b).is_empty());
    }

    #[test]
    fn wide_char_occupies_two_cells_but_renders_once() {
        let mut b = Buffer::new(10, 2);
        b.put_char(1, 0, '🐚', Style::default());
        assert_eq!(b.cell(1, 0).unwrap().ch, '🐚');
        assert!(b.cell(2, 0).unwrap().cont);
        // 导出文本时宽字符只出现一次
        assert_eq!(b.to_text().lines().next().unwrap().chars().filter(|c| *c == '🐚').count(), 1);
        // 与空白比较：emoji 的宽度不确定，所以先擦右半格、再画字符本身
        let blank = Buffer::new(10, 2);
        let patches = b.diff(&blank);
        assert_eq!(patches.len(), 2);
        assert_eq!((patches[0].x, patches[0].cell.ch), (2, ' '));
        assert_eq!((patches[1].x, patches[1].cell.ch), (1, '🐚'));
        // 稳定帧没有任何补丁
        assert!(b.diff(&b).is_empty());
    }

    #[test]
    fn cjk_does_not_waste_a_patch_on_blank_right_half() {
        // 汉字在所有终端上都是 2 列，右半格本来就是空白时不必多擦一次
        let mut b = Buffer::new(10, 1);
        b.put_char(1, 0, '汉', Style::default());
        let patches = b.diff(&Buffer::new(10, 1));
        assert_eq!(patches.len(), 1);
        assert_eq!((patches[0].x, patches[0].cell.ch), (1, '汉'));
    }

    #[test]
    fn repositions_cursor_after_non_ascii_to_avoid_drift() {
        // 终端若把 emoji / 箭头画成 1 列，继续沿用光标位置会让整行逐格左移，
        // 于是弹窗右边框看起来“没闭合”。非 ASCII 之后必须重新 MoveTo。
        let mut r = Renderer::new(6, 1);
        r.buffer_mut().put_char(0, 0, '🐚', Style::default());
        r.buffer_mut().put_str(2, 0, "ab", Style::default());
        let mut out = Vec::new();
        r.present(&mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        // MoveTo(2, 0) => ESC[1;3H
        assert!(
            s.contains("\u{1b}[1;3H"),
            "宽字符之后应重新定位，实际输出: {s:?}"
        );
    }

    #[test]
    fn wide_char_clears_stale_right_half_first() {
        // 右边原本是个边框字符，被宽字符盖住：必须先擦掉再画 emoji，
        // 否则终端若只把 emoji 当 1 列，旧字符就会留在屏幕上。
        let mut old = Buffer::new(5, 1);
        old.put_char(2, 0, '─', Style::default());
        let mut new = Buffer::new(5, 1);
        new.put_char(1, 0, '🐚', Style::default());

        let patches = new.diff(&old);
        assert_eq!(patches.len(), 2);
        assert_eq!((patches[0].x, patches[0].cell.ch), (2, ' '));
        assert_eq!((patches[1].x, patches[1].cell.ch), (1, '🐚'));
    }

    #[test]
    fn overwriting_wide_char_clears_second_half() {
        let mut b = Buffer::new(10, 1);
        b.put_char(1, 0, '🐚', Style::default());
        b.put_char(1, 0, 'A', Style::default());
        assert_eq!(b.cell(1, 0).unwrap().ch, 'A');
        assert!(!b.cell(2, 0).unwrap().cont);
        assert_eq!(b.cell(2, 0).unwrap().ch, ' ');
    }

    #[test]
    fn overwriting_right_half_clears_left_half() {
        let mut b = Buffer::new(10, 1);
        b.put_char(1, 0, '🐚', Style::default());
        b.put_char(2, 0, 'B', Style::default());
        assert_eq!(b.cell(1, 0).unwrap().ch, ' ');
        assert!(!b.cell(1, 0).unwrap().cont);
        assert_eq!(b.cell(2, 0).unwrap().ch, 'B');
    }

    #[test]
    fn never_emits_double_underline() {
        // 旧实现用 `Attribute::NoBold`（SGR 21）关闭加粗，但 21 在 ECMA-48 /
        // Windows Terminal 里表示“双下划线”，会让文字下面出现双横线。
        let mut r = Renderer::new(3, 1);
        r.buffer_mut()
            .put_char(0, 0, 'A', Style::default().bold());
        r.buffer_mut().put_char(1, 0, 'B', Style::default());

        let mut out = Vec::new();
        r.present(&mut out).unwrap();
        let s = String::from_utf8_lossy(&out).to_string();
        assert!(s.contains("\x1b[0m"), "每批补丁开始应先重置属性");
        assert!(s.contains("\x1b[1m"), "加粗应使用 SGR 1");
        assert!(s.contains("\x1b[22m"), "取消加粗应使用 SGR 22");
        assert!(!s.contains("\x1b[21m"), "不能发送 SGR 21（双下划线）");
    }

    #[test]
    fn renderer_full_then_minimal_patch() {
        let mut r = Renderer::new(8, 2);
        r.buffer_mut().put_char(0, 0, 'A', Style::default());
        let mut out = Vec::new();
        assert_eq!(r.present(&mut out).unwrap(), 1);
        let first = String::from_utf8_lossy(&out).to_string();
        assert!(first.contains("\x1b[2J"), "首帧应清屏");

        // 内容不变：没有任何差异
        r.buffer_mut().put_char(0, 0, 'A', Style::default());
        out.clear();
        assert_eq!(r.present(&mut out).unwrap(), 0);
        assert!(out.is_empty());

        // 改一个格子：只出现一次 MoveTo，不再清屏
        r.buffer_mut().put_char(3, 1, 'B', Style::default());
        out.clear();
        assert_eq!(r.present(&mut out).unwrap(), 1);
        let s = String::from_utf8_lossy(&out);
        assert!(!s.contains("\x1b[2J"));
        assert!(s.contains('B'));
    }
}
