//! 界面渲染与布局计算（手机 App 抽屉风格）。
//!
//! 界面不再直接往 stdout 写转义序列，而是：
//! 1. 用 `vdom::Node` 搭出一棵声明式的虚拟树；
//! 2. `VDom` 做带 key 的协调，决定是否需要重画；
//! 3. 树被画进 `buffer::Buffer`（终端虚拟缓冲），最后只把差异刷到终端。


use crate::apps::App;
use crate::buffer::Style;
use crate::icons::{active_icons, render_icon, Picker};
use crate::text::{pad_to_width, str_width, truncate_to_width};
use crate::theme;
use crate::vdom::{Align, Node};

/// 网格里每格的最小宽度（手机风格：一个图标 + 一行名称）。
const MIN_CELL: usize = 8;
/// 最多列数：再宽也不会更密，避免宽窗口变成“桌面”。
const MAX_COLUMNS: usize = 8;
/// 图标之间的水平间距。
const GRID_GAP: usize = 1;
/// 每个应用占用的行数：图标行 + 名称行。
pub const TILE_H: usize = 2;


/// 按终端宽度算出自适应的手机风格网格：每格至少 8 列，最多 8 列。
///
/// 返回 `(列数, 每格宽度)`；列数随窗口变宽而增加，每格宽度基本保持在 9～14 列，
/// 所以窄窗口（手机竖屏）是 4 列，宽窗口最多 8 列，不会把图标拉得很稀。
pub fn grid_metrics(width: usize) -> (usize, usize) {
    let mut columns = ((width + GRID_GAP) / (MIN_CELL + GRID_GAP)).clamp(1, MAX_COLUMNS);
    let mut cell = width.saturating_sub(columns - 1) / columns;
    // 太窄放不下就减列，保证每格至少 MIN_CELL 列
    while cell < MIN_CELL && columns > 1 {
        columns -= 1;
        cell = width.saturating_sub(columns - 1) / columns;
    }
    (columns, cell.max(1))
}

/// 根据终端尺寸计算出的网格布局。
pub struct Layout {
    pub width: u16,
    pub height: u16,
    pub columns: usize,
    /// 每格宽度（图标和名称都在这个宽度里居中）。
    pub cell_w: usize,
    /// 网格左上角坐标（整体水平居中）。
    pub start_x: u16,
    pub start_y: u16,
}

impl Layout {
    pub fn compute(width: u16, height: u16) -> Self {
        let w = width as usize;
        let (columns, cell_w) = grid_metrics(w);
        let grid_w = columns * cell_w + (columns - 1) * GRID_GAP;
        let start_x = w.saturating_sub(grid_w) / 2;
        // 第 0 行是顶栏，第 1 行是命令预览 / 状态提示，网格从第 2 行开始。
        let start_y = 2.min(height as usize);

        Layout {
            width,
            height,
            columns,
            cell_w,
            start_x: start_x as u16,
            start_y: start_y as u16,
        }
    }

    /// 网格总行数。
    pub fn total_rows(&self, count: usize) -> usize {
        if count == 0 {
            1
        } else {
            count.div_ceil(self.columns)
        }
    }

    /// 当前屏幕可显示的行数（顶部预留 2 行、底部预留 1 行底栏）。
    pub fn visible_rows(&self) -> usize {
        let h = self.height as usize;
        let avail = h.saturating_sub(self.start_y as usize + 1);
        (avail / TILE_H).max(1)
    }

    /// 第 `idx` 个图标左上角的坐标。
    pub fn origin(&self, idx: usize, scroll: usize) -> (u16, u16) {
        let col = idx % self.columns;
        let row = idx / self.columns;
        let x = self.start_x + (col * (self.cell_w + GRID_GAP)) as u16;
        let y = self.start_y + ((row.saturating_sub(scroll)) * TILE_H) as u16;
        (x, y)
    }

    /// 命中测试：给定鼠标坐标，返回对应图标下标。
    pub fn hit(&self, x: u16, y: u16, count: usize, scroll: usize) -> Option<usize> {
        let vis = self.visible_rows();
        for i in 0..count {
            let row = i / self.columns;
            if row < scroll || row >= scroll + vis {
                continue;
            }
            let (tx, ty) = self.origin(i, scroll);
            // 命中区包含右侧的间距，点两格之间也算命中
            if x >= tx
                && (x as usize) < tx as usize + self.cell_w + GRID_GAP
                && y >= ty
                && (y as usize) < ty as usize + TILE_H
            {
                return Some(i);
            }
        }
        None
    }
}

/// 保证选中的图标在可视区域内，返回修正后的滚动偏移。
pub fn clamp_scroll(l: &Layout, count: usize, selected: usize, scroll: usize) -> usize {
    let vis = l.visible_rows();
    let total = l.total_rows(count);
    if total <= vis {
        return 0;
    }
    let mut scroll = scroll;
    let sel_row = selected / l.columns;
    if sel_row < scroll {
        scroll = sel_row;
    }
    if sel_row >= scroll + vis {
        scroll = sel_row + 1 - vis;
    }
    scroll.min(total - vis)
}

/// 弹窗中一个输入字段的视图数据。
pub struct FieldView<'a> {
    pub label: &'a str,
    pub text: &'a str,
    /// 光标位置（字符下标）。
    pub cursor: usize,
    pub focused: bool,
}

/// 叠在主界面上的弹窗。
pub enum Overlay<'a> {
    None,
    Edit {
        title: &'a str,
        fields: &'a [FieldView<'a>],
        error: Option<&'a str>,
    },
    Picker {
        picker: &'a Picker,
        current: Option<&'a str>,
    },
    Confirm {
        name: &'a str,
    },
}

/// 构造当前帧的虚拟 DOM，返回（树，输入光标位置）。
pub fn view(
    l: &Layout,
    apps: &[App],
    selected: usize,
    scroll: usize,
    status: Option<&str>,
    overlay: Overlay<'_>,
) -> (Node, Option<(u16, u16)>) {
    let mut root = main_view(l, apps, selected, scroll, status);
    let mut cursor = None;
    match overlay {
        Overlay::None => {}
        Overlay::Edit {
            title,
            fields,
            error,
        } => {
            let (modal, c) = edit_view(l, title, fields, error);
            root = root.child(modal);
            cursor = c;
        }
        Overlay::Picker { picker, current } => {
            root = root.child(picker_view(l, picker, current));
        }
        Overlay::Confirm { name } => {
            root = root.child(confirm_view(l, name));
        }
    }
    (root, cursor)
}

fn main_view(
    l: &Layout,
    apps: &[App],
    selected: usize,
    scroll: usize,
    status: Option<&str>,
) -> Node {
    let w = l.width;
    let h = l.height;
    // 整屏铺底色：亮色主题下窗口本身就是浅色，不靠终端自己的背景
    let mut root = Node::new(0)
        .rect(0, 0, w, h)
        .style(Style::new(theme::fg(), theme::bg()))
        .fill(true);

    // 顶栏
    let title = if crate::icons::ascii_mode() {
        "[:] AppDrawer"
    } else {
        "📱 AppDrawer"
    };
    root = root.child({
        let mut top = Node::new(1)
            .rect(0, 0, w, 1)
            .style(Style::new(theme::on_bar(), theme::bar()).bold())
            .fill(true)
            .text(title)
            .align(Align::Center);
        if !apps.is_empty() {
            top = top.child(
                Node::new(1)
                    .rect(0, 0, w, 1)
                    .style(Style::new(theme::on_bar(), theme::bar()))
                    .text(format!("{}/{} ", selected.saturating_add(1), apps.len()))
                    .align(Align::Right),
            );
        }
        top
    });

    // 选中项命令预览 / 状态提示
    if h > 1 {
        let (text, fg) = match status {
            Some(s) => (
                truncate_to_width(s, (w as usize).saturating_sub(4)),
                theme::warn(),
            ),
            None => match apps.get(selected) {
                Some(app) => (
                    truncate_to_width(&format!("▶ {}", app.command), (w as usize).saturating_sub(4)),
                    theme::hint(),
                ),
                None => (String::new(), theme::hint()),
            },
        };
        root = root.child(
            Node::new(2)
                .rect(0, 1, w, 1)
                .style(Style::new(fg, theme::bg()))
                .fill(true)
                .text(format!("  {}", text)),
        );
    }

    // 图标网格：手机抽屉风格，一格 = 图标行 + 名称行，选中格整块高亮
    let vis = l.visible_rows();
    let cell_w = l.cell_w as u16;
    for (i, app) in apps.iter().enumerate() {
        let row = i / l.columns;
        if row < scroll || row >= scroll + vis {
            continue;
        }
        let (x, y) = l.origin(i, scroll);
        if (y as usize) + TILE_H > (h as usize).saturating_sub(1) {
            continue;
        }
        let selected_tile = i == selected;
        let (bg, fg) = if selected_tile {
            (theme::accent(), theme::on_accent())
        } else {
            (theme::bg(), theme::muted())
        };
        let mut style = Style::new(fg, bg);
        if selected_tile {
            style = style.bold();
        }
        let icon = render_icon(&app.icon);
        let name = cell_label(&app.name, l.cell_w);

        root = root.child(
            Node::new(1000 + i as u64)
                .rect(x, y, cell_w, TILE_H as u16)
                .style(style)
                .fill(true)
                .child(
                    Node::new(1)
                        .rect(x, y, cell_w, 1)
                        .style(style)
                        .text(icon)
                        .align(Align::Center),
                )
                .child(
                    Node::new(2)
                        .rect(x, y + 1, cell_w, 1)
                        .style(style)
                        .text(name)
                        .align(Align::Center),
                ),
        );
    }

    // 底栏：宽度不够时自动退化成短版，保证 a / e / d 始终可见
    if h > 0 {
        root = root.child(
            Node::new(3)
                .rect(0, h - 1, w, 1)
                .style(Style::new(theme::on_bar(), theme::bar()))
                .fill(true)
                .text(footer_text(w as usize))
                .align(Align::Center),
        );
    }

    root
}

/// 名称超出格子宽度时截断并加省略号。
fn cell_label(name: &str, width: usize) -> String {
    if str_width(name) <= width {
        return name.to_string();
    }
    let mut s = truncate_to_width(name, width.saturating_sub(1));
    s.push('…');
    s
}

/// 底栏按键提示：宽度够就用完整版，窄窗口逐级退化成短版。
/// 底栏按键提示：宽度够就用完整文案；窄了先缩文案、再收紧间距，
/// 最后才逐个丢弃——保证只要有地方，每个按键后面都还跟着说明文字。
fn footer_text(width: usize) -> String {
    const FULL: [&str; 7] = [
        "←→↑↓ 选择",
        "Enter 启动",
        "a 添加",
        "e 修改",
        "d 删除",
        "r 刷新",
        "q 退出",
    ];
    const SHORT: [&str; 7] = ["←→ 选择", "Enter", "a 加", "e 改", "d 删", "r 刷", "q 退"];

    // 先挑文案，再挑间距：宁可挤一点，也别把 r 后面的说明吃掉
    for set in [&FULL, &SHORT] {
        for sep in ["   ", "  ", " "] {
            let s = set.join(sep);
            if str_width(&s) <= width {
                return s;
            }
        }
    }
    // 还是太窄：逐个塞，能塞几个算几个
    let mut out = String::new();
    for seg in SHORT {
        let extra = if out.is_empty() { 0 } else { 1 };
        if str_width(&out) + extra + str_width(seg) > width {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(seg);
    }
    out
}

/// 单行输入框的可视窗口：返回显示文本与光标在窗口内的列号。
fn window(chars: &[char], cursor: usize, max: usize) -> (String, usize) {
    if max == 0 {
        return (String::new(), 0);
    }
    let start = if cursor >= max { cursor + 1 - max } else { 0 };
    let start = start.min(chars.len());
    let end = (start + max).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    let used = s.chars().count();
    if used < max {
        s.push_str(&" ".repeat(max - used));
    }
    (s, cursor.saturating_sub(start))
}

fn edit_view(
    l: &Layout,
    title: &str,
    fields: &[FieldView],
    error: Option<&str>,
) -> (Node, Option<(u16, u16)>) {
    let width = l.width as usize;
    let height = l.height as usize;
    let err_rows = if error.is_some() { 1 } else { 0 };

    let mh = (fields.len() + 6 + err_rows)
        .min(height.saturating_sub(2).max(5))
        .max(1) as u16;
    let mut mw_us = width.saturating_sub(4).min(60);
    if mw_us < 24 {
        mw_us = width.min(60);
    }
    let mw = mw_us.max(1) as u16;

    let mx = (width.saturating_sub(mw as usize) / 2) as u16;
    let my = (height.saturating_sub(mh as usize) / 2) as u16;

    let mut modal = Node::new(900)
        .rect(mx, my, mw, mh)
        .style(Style::new(theme::accent(), theme::modal_bg()))
        .fill(true)
        .border(true);

    // 标题
    modal = modal.child(
        Node::new(1)
            .rect(mx.saturating_add(2), my + 1, mw.saturating_sub(4), 1)
            .style(Style::new(theme::modal_fg(), theme::modal_bg()).bold())
            .text(truncate_to_width(title, mw as usize))
            .align(Align::Center),
    );

    let inner_left = mx as usize + 2;
    let field_end = (mx as usize + mw as usize).saturating_sub(2);
    let label_w = fields.iter().map(|f| str_width(f.label)).max().unwrap_or(0);
    let field_x = inner_left + label_w + 2;
    let field_w = field_end.saturating_sub(field_x);

    let mut cursor = None;
    for (i, f) in fields.iter().enumerate() {
        let y = my + 3 + i as u16;

        let label = format!("{}: ", pad_to_width(f.label, label_w));
        modal = modal.child(
            Node::new(10 + i as u64 * 10)
                .rect(inner_left as u16, y, (label_w + 2) as u16, 1)
                .style(Style::new(
                    if f.focused { theme::accent() } else { theme::muted() },
                    theme::modal_bg(),
                ))
                .text(label),
        );

        let chars: Vec<char> = f.text.chars().collect();
        let (vis, cur_col) = window(&chars, f.cursor, field_w);
        let (fbg, ffg) = if f.focused {
            (theme::field_focus(), theme::on_field_focus())
        } else {
            (theme::field(), theme::field_fg())
        };
        modal = modal.child(
            Node::new(11 + i as u64 * 10)
                .rect(field_x as u16, y, field_w as u16, 1)
                .style(Style::new(ffg, fbg))
                .fill(true)
                .text(vis),
        );

        if f.focused {
            cursor = Some(((field_x + cur_col) as u16, y));
        }
    }

    // 错误提示
    if let Some(err) = error {
        let y = my + 3 + fields.len() as u16 + 1;
        let text = truncate_to_width(err, field_end.saturating_sub(inner_left));
        modal = modal.child(
            Node::new(800)
                .rect(
                    inner_left as u16,
                    y,
                    field_end.saturating_sub(inner_left) as u16,
                    1,
                )
                .style(Style::new(theme::danger(), theme::modal_bg()))
                .text(text),
        );
    }

    // 操作提示
    let hint = fit(
        &[
            "Tab 切换   Ctrl+E 选图标   Enter 保存   Esc 取消",
            "Ctrl+E 选图标   Enter 保存   Esc 取消",
            "Enter 保存   Esc 取消",
        ],
        mw.saturating_sub(4) as usize,
    );
    modal = modal.child(
        Node::new(801)
            .rect(mx.saturating_add(2), my + mh - 2, mw.saturating_sub(4), 1)
            .style(Style::new(theme::muted(), theme::modal_bg()))
            .text(hint)
            .align(Align::Center),
    );

    (modal, cursor)
}

/// 图标选择器每行的列数（每个格子宽 4：空格 + 宽 2 的图标 + 空格）。
pub fn picker_columns(l: &Layout) -> usize {
    // 保证 cols*4 + 4（左右边框+留白）不超过终端宽度。
    let max_cols = (l.width as usize).saturating_sub(6) / 4;
    max_cols.clamp(1, 12)
}

/// 从若干候选文案中挑出第一条宽度放得下的（都不行就用最后一条，交给绘制时截断）。
fn fit<'a>(candidates: &[&'a str], width: usize) -> &'a str {
    candidates
        .iter()
        .copied()
        .find(|s| str_width(s) <= width)
        .unwrap_or(candidates[candidates.len() - 1])
}

fn picker_view(l: &Layout, picker: &Picker, current: Option<&str>) -> Node {
    let icons = active_icons();
    let n = icons.len();
    let width = l.width as usize;
    let height = l.height as usize;
    let cols = picker_columns(l);
    let cell_w = 4usize;
    let grid_rows = n.div_ceil(cols);
    // 上下各留 1 行给主界面（顶栏 / 底栏），弹窗自身还要 2 行边框 + 标题 + 提示。
    let rows_visible = grid_rows.min(height.saturating_sub(6).max(1));

    let mw = (cols * cell_w + 4).min(width.max(1)) as u16;
    let mh = (rows_visible + 4).min(height.max(1)).max(5) as u16;
    let mx = (width.saturating_sub(mw as usize) / 2) as u16;
    let my = (height.saturating_sub(mh as usize) / 2) as u16;

    let mut modal = Node::new(900)
        .rect(mx, my, mw, mh)
        .style(Style::new(theme::accent(), theme::modal_bg()))
        .fill(true)
        .border(true);

    // 标题
    let title = format!("选择图标（{} 个）", n);
    modal = modal.child(
        Node::new(1)
            .rect(mx.saturating_add(2), my + 1, mw.saturating_sub(4), 1)
            .style(Style::new(theme::modal_fg(), theme::modal_bg()).bold())
            .text(truncate_to_width(&title, mw as usize))
            .align(Align::Center),
    );

    // 垂直滚动，保证选中项可见
    let sel_row = picker.selected / cols;
    let scroll = if grid_rows <= rows_visible {
        0
    } else {
        let mut s = sel_row.saturating_sub(rows_visible / 2);
        if s + rows_visible > grid_rows {
            s = grid_rows - rows_visible;
        }
        s
    };

    let grid_x = mx as usize + 2;
    let grid_y = my as usize + 2;
    for row in 0..rows_visible {
        let abs = scroll + row;
        for c in 0..cols {
            let idx = abs * cols + c;
            if idx >= n {
                break;
            }
            let y = (grid_y + row) as u16;
            let x = (grid_x + c * cell_w) as u16;
            let icon = icons[idx];
            let is_sel = idx == picker.selected;
            let is_cur = current == Some(icon);
            let (bg, fg) = if is_sel {
                (theme::accent(), theme::on_accent())
            } else if is_cur {
                (theme::field_focus(), theme::on_field_focus())
            } else {
                (theme::modal_bg(), theme::modal_fg())
            };
            let mut style = Style::new(fg, bg);
            if is_sel {
                style = style.bold();
            }
            modal = modal.child(
                Node::new(100 + idx as u64)
                    .rect(x, y, cell_w as u16, 1)
                    .style(style)
                    .fill(true)
                    .text(pad_to_width(&format!(" {} ", icon), cell_w)),
            );
        }
    }

    // 底部：当前预览 + 提示（宽度不够时逐级退化成短版，避免被截断成半截）
    let full = format!(
        "{}   ↑↓←→ 移动   Enter 选择   Esc 取消",
        picker.current()
    );
    let inner_w = mw.saturating_sub(4) as usize;
    let hint = if str_width(&full) <= inner_w {
        full.as_str()
    } else {
        fit(
            &[
                "↑↓←→ 移动   Enter 选择   Esc 取消",
                "Enter 选择   Esc 取消",
                "Enter / Esc",
            ],
            inner_w,
        )
    };
    modal = modal.child(
        Node::new(2)
            .rect(mx.saturating_add(2), my + mh - 2, mw.saturating_sub(4), 1)
            .style(Style::new(theme::muted(), theme::modal_bg()))
            .text(hint)
            .align(Align::Center),
    );

    modal
}

fn confirm_view(l: &Layout, name: &str) -> Node {
    let width = l.width as usize;
    let height = l.height as usize;
    let msg = "删除这个应用？";
    let hint = "y 确认    n / Esc 取消";
    let name = truncate_to_width(name, 40);

    let content = str_width(msg)
        .max(str_width(&name))
        .max(str_width(hint))
        .max(22);
    let mut mw_us = (content + 4).min(width);
    if mw_us < 16 {
        mw_us = width;
    }
    let mw = mw_us.max(1) as u16;
    let mh = 7u16.min(l.height.max(1));

    let mx = (width.saturating_sub(mw as usize) / 2) as u16;
    let my = (height.saturating_sub(mh as usize) / 2) as u16;

    let mut modal = Node::new(900)
        .rect(mx, my, mw, mh)
        .style(Style::new(theme::accent(), theme::modal_bg()))
        .fill(true)
        .border(true);

    modal = modal
        .child(
            Node::new(1)
                .rect(mx.saturating_add(2), my + 2, mw.saturating_sub(4), 1)
                .style(Style::new(theme::modal_fg(), theme::modal_bg()).bold())
                .text(msg)
                .align(Align::Center),
        )
        .child(
            Node::new(2)
                .rect(mx.saturating_add(2), my + 3, mw.saturating_sub(4), 1)
                .style(Style::new(theme::warn(), theme::modal_bg()))
                .text(name)
                .align(Align::Center),
        )
        .child(
            Node::new(3)
                .rect(mx.saturating_add(2), my + 5, mw.saturating_sub(4), 1)
                .style(Style::new(theme::muted(), theme::modal_bg()))
                .text(hint)
                .align(Align::Center),
        );

    modal
}

/// 把虚拟树画进一块全新的缓冲（仅测试用）。
#[cfg(test)]
pub fn render_to_buffer(tree: &Node, width: u16, height: u16) -> crate::buffer::Buffer {
    let mut buf = crate::buffer::Buffer::new(width as usize, height as usize);
    tree.paint(&mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_compact_and_adaptive() {
        // 80 列：8 列 x 9 格，比原来的 5 列 x 12 格密度高一倍
        let l = Layout::compute(80, 24);
        assert_eq!(l.columns, 8);
        assert_eq!(l.cell_w, 9);
        assert_eq!(l.visible_rows(), 10);

        // 手机竖屏宽度（41 列）：4 列，最像手机抽屉
        let phone = Layout::compute(41, 24);
        assert_eq!(phone.columns, 4);
        assert_eq!(phone.cell_w, 9);

        let narrow = Layout::compute(20, 24);
        assert_eq!(narrow.columns, 2);

        // 任何宽度下：格子不会被挤得比 8 列还窄，网格也不会溢出屏幕
        for w in 1u16..=200 {
            let l = Layout::compute(w, 24);
            if l.columns > 1 {
                assert!(l.cell_w >= 8, "宽度 {w} 下每格只有 {} 列", l.cell_w);
            }
            let grid_w = l.columns * l.cell_w + (l.columns - 1);
            assert!(grid_w <= w as usize, "宽度 {w} 下网格 {grid_w} 溢出");
            // 每格窄于终端宽度，图标和名称才有地方放
            assert!(l.cell_w <= w as usize);
        }
    }

    #[test]
    fn hit_maps_mouse_to_index() {
        let l = Layout::compute(80, 24);
        assert_eq!(l.hit(0, 2, 12, 0), Some(0));
        assert_eq!(l.hit(8, 3, 12, 0), Some(0));
        assert_eq!(l.hit(10, 2, 12, 0), Some(1));
        // 第二行（名称行下面是下一行图标）
        assert_eq!(l.hit(0, 4, 12, 0), Some(8));
        // 顶栏 / 状态行不算命中
        assert_eq!(l.hit(0, 0, 12, 0), None);
        assert_eq!(l.hit(0, 1, 12, 0), None);
    }

    #[test]
    fn scroll_keeps_selection_visible() {
        let l = Layout::compute(40, 10);
        let s = clamp_scroll(&l, 12, 11, 0);
        assert_eq!(s, l.total_rows(12) - l.visible_rows());

        // 矮窗口：12 个应用只放得下 2 行，选中最后一行时要滚动
        let l = Layout::compute(40, 8);
        assert_eq!(l.visible_rows(), 2);
        assert_eq!(clamp_scroll(&l, 12, 11, 0), 1);
    }

    #[test]
    fn footer_degrades_but_keeps_aed_hints() {
        let wide = footer_text(80);
        assert!(wide.contains("a 添加") && wide.contains("e 修改") && wide.contains("d 删除"));
        assert!(wide.contains("r 刷新") && wide.contains("q 退出"));

        // 紧凑窗口（41 列，手机抽屉）：自动换成短版，但每个键后面都还有说明
        let compact = footer_text(41);
        assert!(str_width(&compact) <= 41, "紧凑底栏 {compact:?} 超出 41 列");
        for want in ["←→ 选择", "Enter", "a 加", "e 改", "d 删", "r 刷", "q 退"] {
            assert!(compact.contains(want), "紧凑底栏 {compact:?} 缺少 {want:?}");
        }

        // 宽度 >= 38 时，每个键后面都还有说明（不出现“光杆 r”那种碎片）
        let labeled = [
            ("a 加", "a 添加"),
            ("e 改", "e 修改"),
            ("d 删", "d 删除"),
            ("r 刷", "r 刷新"),
            ("q 退", "q 退出"),
        ];
        for w in 38..=140 {
            let t = footer_text(w);
            for (short, full) in labeled {
                assert!(
                    t.contains(short) || t.contains(full),
                    "宽度 {w} 下底栏 {t:?} 缺少 {short:?}/{full:?}"
                );
            }
        }

        for w in 8..=140 {
            assert!(str_width(&footer_text(w)) <= w, "宽度 {w} 下底栏溢出");
        }
    }

    #[test]
    fn long_names_are_truncated_with_ellipsis() {
        assert_eq!(cell_label("Shell", 9), "Shell");
        assert_eq!(cell_label("一个很长的应用名字", 9), "一个很长…");
        assert_eq!(str_width(&cell_label("abcdefghijklmn", 9)), 9);
    }

    #[test]
    fn browse_view_contains_apps() {
        let apps = crate::apps::defaults();
        let l = Layout::compute(80, 24);
        let (tree, cursor) = view(&l, &apps, 0, 0, None, Overlay::None);
        let buf = render_to_buffer(&tree, 80, 24);
        let s = buf.to_text();
        assert!(cursor.is_none());
        assert!(s.contains("AppDrawer"));
        assert!(s.contains("Shell"));
        assert!(s.contains("bash"));
    }

    #[test]
    fn status_overrides_preview_line() {
        let apps = crate::apps::defaults();
        let l = Layout::compute(80, 24);
        let (tree, _) = view(&l, &apps, 0, 0, Some("已启动「Shell」"), Overlay::None);
        let buf = render_to_buffer(&tree, 80, 24);
        assert!(buf.to_text().contains("已启动「Shell」"));
    }

    #[test]
    fn edit_modal_renders_and_returns_cursor() {
        let l = Layout::compute(80, 24);
        let fields = [
            FieldView {
                label: "图标",
                text: "🐚",
                cursor: 1,
                focused: false,
            },
            FieldView {
                label: "名称",
                text: "Shell",
                cursor: 5,
                focused: true,
            },
            FieldView {
                label: "命令",
                text: "bash",
                cursor: 4,
                focused: false,
            },
        ];
        let (tree, cursor) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Edit {
                title: "添加应用",
                fields: &fields,
                error: None,
            },
        );
        let buf = render_to_buffer(&tree, 80, 24);
        let s = buf.to_text();
        assert!(cursor.is_some());
        assert!(s.contains("添加应用"));
        assert!(s.contains("名称"));
        assert!(s.contains("Shell"));
    }

    #[test]
    fn edit_modal_shows_error() {
        let l = Layout::compute(80, 24);
        let fields = [
            FieldView {
                label: "图标",
                text: "",
                cursor: 0,
                focused: true,
            },
            FieldView {
                label: "名称",
                text: "",
                cursor: 0,
                focused: false,
            },
            FieldView {
                label: "命令",
                text: "",
                cursor: 0,
                focused: false,
            },
        ];
        let (tree, _) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Edit {
                title: "添加应用",
                fields: &fields,
                error: Some("名称不能为空"),
            },
        );
        let buf = render_to_buffer(&tree, 80, 24);
        assert!(buf.to_text().contains("名称不能为空"));
    }

    #[test]
    fn confirm_modal_contains_name() {
        let l = Layout::compute(80, 24);
        let (tree, _) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Confirm { name: "Files" },
        );
        let buf = render_to_buffer(&tree, 80, 24);
        let s = buf.to_text();
        assert!(s.contains("Files"));
        assert!(s.contains("删除"));
    }

    #[test]
    fn picker_modal_renders_icons_and_preview() {
        let l = Layout::compute(80, 24);
        let p = Picker::new();
        let (tree, _) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Picker {
                picker: &p,
                current: Some("🐚"),
            },
        );
        let buf = render_to_buffer(&tree, 80, 24);
        let s = buf.to_text();
        assert!(s.contains("选择图标"));
        assert!(s.contains(active_icons()[0]));
        assert!(s.contains(active_icons()[1]));
        assert!(s.contains("Enter 选择"));
    }

    #[test]
    fn picker_columns_never_overflow_width() {
        for w in 8u16..=120 {
            let l = Layout::compute(w, 24);
            let cols = picker_columns(&l);
            assert!(cols >= 1, "宽度 {w} 下列数至少为 1");
            assert!(
                cols * 4 + 4 <= w as usize,
                "宽度 {w} 下 {cols} 列会超出弹窗"
            );
        }
    }
}

#[cfg(test)]
mod border_tests {
    use super::*;
    use crate::buffer::Buffer;

    fn find_corner(buf: &Buffer) -> Option<(u16, u16, u16)> {
        for y in 0..buf.height as u16 {
            for x in 0..buf.width as u16 {
                if buf.cell(x, y).map(|c| c.ch) == Some('╭') {
                    let mut rx = x + 1;
                    while rx < buf.width as u16 {
                        if buf.cell(rx, y).map(|c| c.ch) == Some('╮') {
                            return Some((x, y, rx - x + 1));
                        }
                        rx += 1;
                    }
                }
            }
        }
        None
    }

    /// 弹窗四边必须完整：任何文字都不能盖住边框。
    fn assert_box_closed(buf: &Buffer, label: &str) {
        let (x, y, w) = find_corner(buf).unwrap_or_else(|| panic!("{label}: 找不到弹窗边框"));
        let mut row = 0u16;
        loop {
            let yy = y + row;
            assert!(yy < buf.height as u16, "{label}: 弹窗没有底边");
            let left = buf.cell(x, yy).unwrap().ch;
            let right = buf.cell(x + w - 1, yy).unwrap().ch;
            assert!(
                matches!(left, '│' | '╭' | '╰'),
                "{label}: 第 {yy} 行左边框被覆盖成 {left:?}"
            );
            assert!(
                matches!(right, '│' | '╮' | '╯'),
                "{label}: 第 {yy} 行右边框被覆盖成 {right:?}"
            );
            if left == '╰' {
                assert_eq!(right, '╯', "{label}: 缺少右下角");
                break;
            }
            row += 1;
        }
    }

    fn fields() -> [FieldView<'static>; 3] {
        [
            FieldView {
                label: "图标",
                text: "🐚",
                cursor: 1,
                focused: true,
            },
            FieldView {
                label: "名称",
                text: "Subshell window",
                cursor: 5,
                focused: false,
            },
            FieldView {
                label: "命令",
                text: "bash -l",
                cursor: 4,
                focused: false,
            },
        ]
    }

    #[test]
    fn picker_border_survives_any_size() {
        for (w, h) in [
            (20u16, 8u16),
            (30, 12),
            (40, 16),
            (41, 13), // 手机抽屉紧凑窗口
            (60, 20),
            (80, 24),
            (120, 30),
        ] {
            let l = Layout::compute(w, h);
            let p = Picker::new();
            let (tree, _) = view(
                &l,
                &[],
                0,
                0,
                None,
                Overlay::Picker {
                    picker: &p,
                    current: Some("🐚"),
                },
            );
            assert_box_closed(&render_to_buffer(&tree, w, h), &format!("picker {w}x{h}"));
        }
    }

    #[test]
    fn light_theme_paints_every_cell() {
        use crossterm::style::Color;
        // 深色主题下底色就是终端自己的背景（Reset），这条不适用
        if !crate::theme::is_light() {
            return;
        }
        let apps = [App {
            icon: "🐚".into(),
            name: "Shell".into(),
            command: "bash".into(),
        }];
        for (w, h) in [(41u16, 13u16), (80, 24), (20, 8)] {
            let l = Layout::compute(w, h);
            let (tree, _) = view(&l, &apps, 0, 0, None, Overlay::None);
            let buf = render_to_buffer(&tree, w, h);
            for y in 0..h {
                for x in 0..w {
                    let cell = buf.cell(x, y).expect("cell");
                    assert_ne!(
                        (cell.style.bg, cell.style.fg),
                        (Color::Reset, Color::Reset),
                        "({x},{y}) 还是终端默认底色"
                    );
                }
            }
        }
    }

    #[test]
    fn long_text_never_covers_modal_border() {
        let l = Layout::compute(34, 14);
        let (tree, _) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Confirm {
                name: "一个特别特别特别长的应用名字用来测试截断",
            },
        );
        assert_box_closed(&render_to_buffer(&tree, 34, 14), "confirm");

        let f = fields();
        let (tree, _) = view(
            &l,
            &[],
            0,
            0,
            None,
            Overlay::Edit {
                title: "添加应用",
                fields: &f,
                error: Some("三个字段都不能为空"),
            },
        );
        assert_box_closed(&render_to_buffer(&tree, 34, 14), "edit");
    }
}
