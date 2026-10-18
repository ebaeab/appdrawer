//! 一个极简的 React 风格“虚拟 DOM”。
//!
//! 每一帧把界面描述成一棵树（`Node`），交给 `VDom`：
//! * 若新树和上一棵树在**带 key 的协调**下完全一致，就跳过绘制；
//! * 否则把新树整体画进终端虚拟缓冲（`crate::buffer::Buffer`），
//!   再由 `Renderer` 做单元格级 diff，只向终端写变化的部分。
//!
//! 这对应 React 的两层思路：virtual DOM（这棵树）+ 最小化真实 DOM 更新
//! （这里的“真实 DOM”就是终端缓冲区）。

use crate::buffer::{Buffer, Rect, Style};
use crate::text::{str_width, truncate_to_width};

/// 文本在矩形内的水平对齐方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// 虚拟 DOM 节点。
///
/// `key` 只需在同一父节点的子节点之间唯一；协调算法按 key 匹配新旧兄弟节点。
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Node {
    pub key: u64,
    pub rect: Rect,
    pub style: Style,
    pub text: Option<String>,
    pub align: Align,
    /// 绘制前先用 `style` 填充整个矩形（背景 / 覆盖旧内容）。
    pub fill: bool,
    /// 用 `style.fg` 画一个边框（`╭─╮│╰╯`）。
    pub border: bool,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(key: u64) -> Self {
        Self {
            key,
            ..Self::default()
        }
    }

    pub fn rect(mut self, x: u16, y: u16, w: u16, h: u16) -> Self {
        self.rect = Rect::new(x, y, w, h);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn fill(mut self, yes: bool) -> Self {
        self.fill = yes;
        self
    }

    pub fn border(mut self, yes: bool) -> Self {
        self.border = yes;
        self
    }

    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// 把本节点及其子树画进虚拟缓冲。
    pub fn paint(&self, buf: &mut Buffer) {
        if self.fill && !self.rect.is_empty() {
            buf.fill(self.rect, self.style);
        }
        if self.border {
            paint_border(buf, self.rect, self.style);
        }
        if let Some(text) = &self.text {
            let text = truncate_to_width(text, self.rect.w as usize);
            let tw = str_width(&text);
            let x = match self.align {
                Align::Left => self.rect.x,
                Align::Center => {
                    self.rect
                        .x
                        .saturating_add(((self.rect.w as usize).saturating_sub(tw) / 2) as u16)
                }
                Align::Right => self
                    .rect
                    .x
                    .saturating_add((self.rect.w as usize).saturating_sub(tw) as u16),
            };
            buf.put_str(x, self.rect.y, &text, self.style);
        }
        for child in &self.children {
            child.paint(buf);
        }
    }
}

fn paint_border(buf: &mut Buffer, r: Rect, style: Style) {
    if r.w < 2 || r.h < 2 {
        return;
    }
    let border_style = Style {
        fg: style.fg,
        bg: style.bg,
        bold: style.bold,
    };
    let inner = r.w - 2;

    buf.put_char(r.x, r.y, '╭', border_style);
    for i in 0..inner {
        buf.put_char(r.x + 1 + i, r.y, '─', border_style);
    }
    buf.put_char(r.right() - 1, r.y, '╮', border_style);

    for dy in 1..r.h.saturating_sub(1) {
        buf.put_char(r.x, r.y + dy, '│', border_style);
        buf.put_char(r.right() - 1, r.y + dy, '│', border_style);
    }

    buf.put_char(r.x, r.bottom() - 1, '╰', border_style);
    for i in 0..inner {
        buf.put_char(r.x + 1 + i, r.bottom() - 1, '─', border_style);
    }
    buf.put_char(r.right() - 1, r.bottom() - 1, '╯', border_style);
}

/// 协调结果中的一项变化。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    Added(u64),
    Removed(u64),
    Changed(u64),
}

/// 比较两棵虚拟树，返回差异（React 式按 key 协调）。
pub fn diff(old: &Node, new: &Node) -> Vec<Change> {
    let mut out = Vec::new();
    diff_node(old, new, &mut out);
    out
}

fn diff_node(old: &Node, new: &Node, out: &mut Vec<Change>) {
    if old.key != new.key {
        out.push(Change::Removed(old.key));
        out.push(Change::Added(new.key));
        return;
    }

    // 节点自身的属性变了：整棵子树都视为脏的，无需继续递归。
    if old.rect != new.rect
        || old.style != new.style
        || old.text != new.text
        || old.align != new.align
        || old.fill != new.fill
        || old.border != new.border
    {
        out.push(Change::Changed(new.key));
        return;
    }

    for new_child in &new.children {
        match old.children.iter().find(|c| c.key == new_child.key) {
            Some(old_child) => diff_node(old_child, new_child, out),
            None => out.push(Change::Added(new_child.key)),
        }
    }
    for old_child in &old.children {
        if !new.children.iter().any(|c| c.key == old_child.key) {
            out.push(Change::Removed(old_child.key));
        }
    }
}

/// 持有上一帧虚拟树的协调器。
#[derive(Default)]
pub struct VDom {
    prev: Option<Node>,
    dirty: bool,
}

impl VDom {
    pub fn new() -> Self {
        Self {
            prev: None,
            dirty: false,
        }
    }

    /// 强制下一帧重绘（终端尺寸变化、布局结构性变化时调用）。
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// 提交新树并绘制。返回是否真的发生了绘制。
    ///
    /// 若返回 `false`，`buf` 不会被修改，调用方可以跳过 `present()`。
    pub fn render(&mut self, next: Node, buf: &mut Buffer) -> bool {
        let changed = self.dirty
            || match &self.prev {
                None => true,
                Some(prev) => !diff(prev, &next).is_empty(),
            };
        self.dirty = false;
        if !changed {
            return false;
        }
        buf.clear();
        next.paint(buf);
        self.prev = Some(next);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Color;

    fn text_node(key: u64, text: &str) -> Node {
        Node::new(key).rect(0, 0, 10, 1).text(text)
    }

    #[test]
    fn unchanged_tree_produces_no_changes() {
        let a = text_node(1, "hi");
        let b = text_node(1, "hi");
        assert!(diff(&a, &b).is_empty());
    }

    #[test]
    fn changed_text_is_detected() {
        let a = text_node(1, "hi");
        let b = text_node(1, "bye");
        assert_eq!(diff(&a, &b), vec![Change::Changed(1)]);
    }

    #[test]
    fn keyed_children_detect_add_remove_and_reorder() {
        let a = Node::new(0)
            .child(text_node(1, "a"))
            .child(text_node(2, "b"));
        let b = Node::new(0)
            .child(text_node(2, "b"))
            .child(text_node(1, "a"));
        // 只是换了顺序：每个节点内容不变，应无差异
        assert!(diff(&a, &b).is_empty());

        let c = Node::new(0).child(text_node(1, "a"));
        assert_eq!(diff(&a, &c), vec![Change::Removed(2)]);

        let d = Node::new(0)
            .child(text_node(1, "a"))
            .child(text_node(3, "c"));
        assert_eq!(
            diff(&a, &d),
            vec![Change::Added(3), Change::Removed(2)]
        );
    }

    #[test]
    fn vdom_skips_paint_when_unchanged() {
        let mut vdom = VDom::new();
        let mut buf = Buffer::new(20, 1);
        assert!(vdom.render(text_node(1, "hi"), &mut buf));
        assert_eq!(buf.to_text().trim_end(), "hi");

        assert!(!vdom.render(text_node(1, "hi"), &mut buf));
        assert!(vdom.render(text_node(1, "bye"), &mut buf));
        assert_eq!(buf.to_text().trim_end(), "bye");
    }

    #[test]
    fn paint_centers_and_borders() {
        let mut buf = Buffer::new(10, 3);
        let node = Node::new(0)
            .rect(0, 0, 10, 3)
            .style(Style::new(Color::White, Color::Black))
            .fill(true)
            .border(true)
            .child(Node::new(1).rect(0, 1, 10, 1).text("ab").align(Align::Center));
        node.paint(&mut buf);
        let text = buf.to_text();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with('╭'));
        assert!(lines[0].ends_with('╮'));
        assert!(lines[2].starts_with('╰'));
        assert!(lines[1].contains("│   ab   │"));
    }
}
