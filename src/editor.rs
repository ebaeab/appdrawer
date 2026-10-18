//! 应用编辑器：用于在界面内添加 / 修改应用信息。

use crate::apps::App;

/// 单行文本输入框（以字符为单位维护光标）。
pub struct Field {
    pub chars: Vec<char>,
    pub cursor: usize,
}

impl Field {
    pub fn new(s: &str) -> Self {
        let chars: Vec<char> = s.chars().collect();
        let cursor = chars.len();
        Field { chars, cursor }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }
}

/// 编辑对话框状态。
pub struct EditState {
    /// 依次为：图标、名称、命令。
    pub fields: [Field; 3],
    /// 当前聚焦的字段下标。
    pub focus: usize,
    /// `None` 表示新增；`Some(i)` 表示修改第 i 个应用。
    pub editing_index: Option<usize>,
    /// 校验错误提示。
    pub error: Option<String>,
}

impl EditState {
    /// 新建应用。
    pub fn new_app() -> Self {
        EditState {
            fields: [Field::new(""), Field::new(""), Field::new("")],
            focus: 0,
            editing_index: None,
            error: None,
        }
    }

    /// 修改已有应用。
    pub fn edit(app: &App, index: usize) -> Self {
        EditState {
            fields: [
                Field::new(&app.icon),
                Field::new(&app.name),
                Field::new(&app.command),
            ],
            focus: 0,
            editing_index: Some(index),
            error: None,
        }
    }

    /// 切换字段（正/反向）。
    pub fn cycle(&mut self, forward: bool) {
        let n = self.fields.len();
        self.focus = if forward {
            (self.focus + 1) % n
        } else {
            (self.focus + n - 1) % n
        };
        self.error = None;
    }

    /// 校验并转换为 `App`。
    pub fn to_app(&self) -> Result<App, String> {
        let name = self.fields[1].text().trim().to_string();
        let command = self.fields[2].text().trim().to_string();
        let icon = self.fields[0].text().trim().to_string();

        if name.is_empty() {
            return Err("名称不能为空".to_string());
        }
        if command.is_empty() {
            return Err("命令不能为空".to_string());
        }
        Ok(App {
            icon: if icon.is_empty() { "▫".to_string() } else { icon },
            name,
            command,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_edit_operations() {
        let mut f = Field::new("ab");
        assert_eq!(f.cursor, 2);
        f.insert('c');
        assert_eq!(f.text(), "abc");
        f.backspace();
        assert_eq!(f.text(), "ab");
        f.home();
        f.delete();
        assert_eq!(f.text(), "b");
        f.end();
        f.left();
        f.insert('X');
        assert_eq!(f.text(), "Xb");
    }

    #[test]
    fn validation_requires_name_and_command() {
        let mut e = EditState::new_app();
        assert!(e.to_app().is_err());
        e.fields[1] = Field::new("MyApp");
        assert!(e.to_app().is_err());
        e.fields[2] = Field::new("echo hi");
        let app = e.to_app().unwrap();
        assert_eq!(app.name, "MyApp");
        assert_eq!(app.command, "echo hi");
        assert_eq!(app.icon, "▫");
    }

    #[test]
    fn edit_prefills_existing_app() {
        let app = App {
            icon: "🐚".to_string(),
            name: "Shell".to_string(),
            command: "bash".to_string(),
        };
        let e = EditState::edit(&app, 2);
        assert_eq!(e.editing_index, Some(2));
        assert_eq!(e.fields[0].text(), "🐚");
        assert_eq!(e.fields[1].text(), "Shell");
        assert_eq!(e.fields[2].text(), "bash");
    }
}
