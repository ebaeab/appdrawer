//! AppDrawer —— 手机 App 抽屉风格的交互式 TUI 启动器。
//!
//! 功能：
//! * 全屏、鼠标 + 键盘操作的图标网格；
//! * 方向键 / hjkl 移动，Enter 或鼠标单击启动应用；
//! * 每个按钮都在**新窗口 / 新会话**中启动（Termux 新会话、Windows 新控制台、
//!   macOS 终端、Linux 终端模拟器），AppDrawer 本身不退出、可重复启动；
//! * `a` 添加、`e` 修改、`d` 删除应用，改动会写回配置文件；
//! * `r` 重新加载配置文件，`q`/`Esc` 退出。

mod apps;
mod buffer;
mod editor;
mod icons;
mod launcher;
mod log;
mod text;
mod theme;
mod ui;
mod vdom;
mod winmove;

use std::io::{self, Write};
use std::panic;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyModifiers, MouseButton, MouseEventKind,
    },
    execute, queue,
    style::{Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};

use apps::App;
use buffer::Renderer;
use editor::EditState;
use icons::Picker;
use ui::{FieldView, Layout};
use vdom::VDom;

/// 界面模式。
enum Mode {
    /// 浏览图标网格。
    Browse,
    /// 编辑应用信息。
    Edit(EditState),
    /// 选择内置图标（携带编辑状态以便返回）。
    IconPicker(EditState, Picker),
    /// 删除确认。
    Confirm(usize),
}

/// 事件处理结果。
enum Next {
    /// 切换到某个模式继续运行。
    Mode(Mode),
    /// 退出程序。
    Quit,
}

fn main() -> io::Result<()> {
    // 第一件事：双击启动时那个一瞬而过的控制台窗口，能藏就藏（见函数注释）。
    launcher::hide_startup_console();
    log::log(&format!(
        "=== AppDrawer 启动 | exe={:?} | pid={} | log={:?}",
        std::env::current_exe().ok(),
        std::process::id(),
        log::path()
    ));
    // 一行记下所有相关环境变量（没设的记 `-`），排查时不用挨个猜。
    let env_summary: Vec<String> = [
        "WT_SESSION",
        "ConEmuANSI",
        "WEZTERM_EXECUTABLE",
        "APPDRAWER_THEME",
        "APPDRAWER_ASCII",
        "APPDRAWER_USE_WT",
        "APPDRAWER_NO_WT",
        "APPDRAWER_IN_WT",
        "APPDRAWER_NO_RELAUNCH",
        "APPDRAWER_BORDERLESS",
        "APPDRAWER_IN_BORDERLESS",
        "APPDRAWER_WT_FULLSCREEN",
        "APPDRAWER_WINDOW_POS",
        "APPDRAWER_CONFIG",
        "APPDRAWER_LOG",
    ]
    .iter()
    .map(|key| match std::env::var(key) {
        Ok(v) => format!("{key}={v:?}"),
        Err(_) => format!("{key}=-"),
    })
    .collect();
    log::log(&format!("env: {}", env_summary.join(" ")));

    // 主题要先定：无边框重启时要把窗口背景色一并传给终端，
    // 否则格子之外那一圈（内边距 / 取整像素）还是终端自己的深色 → 黑边。
    theme::init();

    // 先读配置（也为了知道应用数量，好估出一个紧凑窗口大小）。
    let mut list = apps::load();
    if list.is_empty() {
        list = apps::defaults();
    }

    // Windows：可选地在 Windows Terminal 中重启（默认开启，设 APPDRAWER_USE_WT=0 关闭）。
    // 设 APPDRAWER_BORDERLESS=1 时改为在 WezTerm / Alacritty 里开无边框窗口。
    if launcher::relaunch_in_modern_terminal(list.len()) {
        log::log("已在新终端中重启，退出当前进程");
        return Ok(());
    }
    // 换窗口失败：要是刚把控制台藏了，现在得还回来，不然界面看不见。
    launcher::restore_console_if_hidden();
    // Windows：切到 UTF-8 代码页，避免中文 / emoji 乱码（其它平台为空操作）。
    launcher::enable_utf8_console();

    // 旧版控制台（双击 exe 打开的 conhost）无法渲染彩色 emoji，自动改用纯 ASCII 字符图标。
    // 可用 APPDRAWER_ASCII=1 强制 ASCII，APPDRAWER_ASCII=0 强制 emoji。
    let ascii_env = std::env::var("APPDRAWER_ASCII")
        .ok()
        .map(|v| v.to_ascii_lowercase());
    let ascii = match ascii_env.as_deref() {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        _ => launcher::legacy_console(),
    };
    icons::set_ascii_mode(ascii);
    log::log(&format!("ascii_mode = {ascii}"));
    // Windows：本进程按物理像素工作，否则高 DPI 下 `--position` 和拖动都会偏。
    launcher::prepare_dpi();

    install_panic_hook();
    launcher::cleanup_old_scripts();

    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        Hide,
        // 标题同时是 winmove 找宿主窗口的依据（无边框窗口没有标题条，但窗口本身有标题）
        SetTitle("AppDrawer"),
        SetBackgroundColor(theme::bg()),
        SetForegroundColor(theme::fg())
    )?;

    let result = run(&mut out, &mut list);

    // 恢复终端
    let _ = execute!(
        out,
        Show,
        LeaveAlternateScreen,
        DisableMouseCapture,
        SetBackgroundColor(Color::Reset),
        SetForegroundColor(Color::Reset),
        SetAttribute(Attribute::Reset)
    );
    let _ = terminal::disable_raw_mode();

    result
}

/// 主事件循环。
fn run(out: &mut io::Stdout, list: &mut Vec<App>) -> io::Result<()> {
    let (mut width, mut height) = terminal::size()?;
    let mut renderer = Renderer::new(width as usize, height as usize);
    let mut vdom = VDom::new();
    let mut selected = 0usize;
    let mut scroll = 0usize;
    let mut status: Option<String> = None;
    let mut mode = Mode::Browse;
    // 左键按下的位置：抬起时才算点击，中途变成拖动就不启动应用
    let mut press: Option<(u16, u16)> = None;

    loop {
        let layout = Layout::compute(width, height);
        scroll = ui::clamp_scroll(&layout, list.len(), selected, scroll);

        // 1) 用虚拟 DOM 描述这一帧，2) 协调后画进终端虚拟缓冲。
        let (tree, cursor) = match &mode {
            Mode::Browse => ui::view(
                &layout,
                list,
                selected,
                scroll,
                status.as_deref(),
                ui::Overlay::None,
            ),
            Mode::Edit(e) => {
                let texts = [e.fields[0].text(), e.fields[1].text(), e.fields[2].text()];
                let labels = ["图标", "名称", "命令"];
                let views: Vec<FieldView> = (0..3)
                    .map(|i| FieldView {
                        label: labels[i],
                        text: &texts[i],
                        cursor: e.fields[i].cursor,
                        focused: i == e.focus,
                    })
                    .collect();
                let title = if e.editing_index.is_some() {
                    "编辑应用"
                } else {
                    "添加应用"
                };
                ui::view(
                    &layout,
                    list,
                    selected,
                    scroll,
                    status.as_deref(),
                    ui::Overlay::Edit {
                        title,
                        fields: &views,
                        error: e.error.as_deref(),
                    },
                )
            }
            Mode::IconPicker(e, p) => {
                let current = e.fields[0].text();
                ui::view(
                    &layout,
                    list,
                    selected,
                    scroll,
                    status.as_deref(),
                    ui::Overlay::Picker {
                        picker: p,
                        current: Some(current.as_str()),
                    },
                )
            }
            Mode::Confirm(i) => {
                let name = list
                    .get(*i)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                ui::view(
                    &layout,
                    list,
                    selected,
                    scroll,
                    status.as_deref(),
                    ui::Overlay::Confirm { name: &name },
                )
            }
        };

        // 3) 只把缓冲区里真正变化的单元格写到终端。
        if vdom.render(tree, renderer.buffer_mut()) {
            renderer.present(out)?;
        }

        // 仅在编辑模式下显示输入光标
        match cursor {
            Some((x, y)) => queue!(out, MoveTo(x, y), Show)?,
            None => queue!(out, Hide)?,
        }
        out.flush()?;

        // 读取并分发事件
        let ev = event::read()?;
        status = None;
        if let Event::Resize(w, h) = &ev {
            width = *w;
            height = *h;
            renderer.resize(*w as usize, *h as usize);
            vdom.invalidate();
            continue;
        }
        // Alt + 鼠标拖动无边框窗口：任何模式下都先让拖动逻辑看一眼
        if let Event::Mouse(m) = &ev
            && let MouseEventKind::Drag(MouseButton::Left) = m.kind
            && winmove::active()
        {
            winmove::follow();
            continue;
        }

        mode = match mode {
            Mode::Browse => match handle_browse(
                ev,
                list,
                &mut selected,
                &mut status,
                &layout,
                scroll,
                &mut press,
            )? {
                Next::Quit => break,
                Next::Mode(m) => m,
            },
            Mode::Edit(e) => match handle_edit(ev, e, list, &mut selected, &mut status)? {
                Next::Quit => break,
                Next::Mode(m) => m,
            },
            Mode::IconPicker(e, p) => match handle_picker(ev, e, p, &layout)? {
                Next::Quit => break,
                Next::Mode(m) => m,
            },
            Mode::Confirm(i) => match handle_confirm(ev, i, list, &mut selected, &mut status)? {
                Next::Quit => break,
                Next::Mode(m) => m,
            },
        };
    }

    Ok(())
}

/// 浏览模式的事件处理。
fn handle_browse(
    ev: Event,
    list: &mut Vec<App>,
    selected: &mut usize,
    status: &mut Option<String>,
    layout: &Layout,
    scroll: usize,
    press: &mut Option<(u16, u16)>,
) -> io::Result<Next> {
    match ev {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Next::Quit);
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Next::Quit),
                KeyCode::Char('r') => {
                    let fresh = apps::load();
                    if !fresh.is_empty() {
                        *list = fresh;
                        *selected = (*selected).min(list.len().saturating_sub(1));
                        *status = Some("已重新加载配置".to_string());
                    }
                }
                KeyCode::Char('a') => {
                    return Ok(Next::Mode(Mode::Edit(EditState::new_app())));
                }
                KeyCode::Char('e') => {
                    if let Some(app) = list.get(*selected) {
                        return Ok(Next::Mode(Mode::Edit(EditState::edit(app, *selected))));
                    }
                }
                KeyCode::Char('d') => {
                    if !list.is_empty() {
                        return Ok(Next::Mode(Mode::Confirm(*selected)));
                    }
                }
                KeyCode::Enter => {
                    if let Some(app) = list.get(*selected) {
                        let (name, cmd) = (app.name.clone(), app.command.clone());
                        *status = Some(launch_status(&name, &cmd));
                    }
                }
                // 方向键：格子越界的情况不处理（守卫不成立就落到 `_ => {}`）
                KeyCode::Left | KeyCode::Char('h') if !(*selected).is_multiple_of(layout.columns) => {
                    *selected -= 1;
                }
                KeyCode::Right | KeyCode::Char('l')
                    if *selected % layout.columns + 1 < layout.columns
                        && *selected + 1 < list.len() =>
                {
                    *selected += 1;
                }
                KeyCode::Up | KeyCode::Char('k') if *selected >= layout.columns => {
                    *selected -= layout.columns;
                }
                KeyCode::Down | KeyCode::Char('j') if *selected + layout.columns < list.len() => {
                    *selected += layout.columns;
                }
                _ => {}
            }
        }
        Event::Mouse(mouse) => {
            // 拖动（按住左键在窗口里滑动）= 搬窗口；原地按下抬起 = 启动应用。
            // 阈值 1 格：少数终端会把轻微抖动报成 Drag，不能因此吃掉点击。
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    *press = Some((mouse.column, mouse.row));
                    if mouse.modifiers.contains(KeyModifiers::ALT) && winmove::begin() {
                        *press = None;
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if winmove::active() {
                        winmove::follow();
                    } else if let Some((cx, cy)) = *press {
                        let moved = mouse.column.abs_diff(cx) > 1 || mouse.row.abs_diff(cy) > 1;
                        if moved {
                            *press = None;
                            if winmove::begin() {
                                winmove::follow();
                                *status = Some("拖动窗口…".to_string());
                            }
                        }
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if winmove::active() {
                        winmove::end();
                        *status = None;
                    } else if let Some((x, y)) = press.take()
                        && let Some(i) = layout.hit(x, y, list.len(), scroll)
                    {
                        *selected = i;
                        let (name, cmd) = (list[i].name.clone(), list[i].command.clone());
                        *status = Some(launch_status(&name, &cmd));
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(Next::Mode(Mode::Browse))
}

/// 编辑模式的事件处理。
fn handle_edit(
    ev: Event,
    mut e: EditState,
    list: &mut Vec<App>,
    selected: &mut usize,
    status: &mut Option<String>,
) -> io::Result<Next> {
    let Event::Key(key) = ev else {
        return Ok(Next::Mode(Mode::Edit(e)));
    };
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Ok(Next::Mode(Mode::Edit(e)));
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(Next::Quit);
    }

    // Ctrl+E：打开内置图标选择器
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('e') {
        return Ok(Next::Mode(Mode::IconPicker(e, Picker::new())));
    }

    match key.code {
        KeyCode::Esc => return Ok(Next::Mode(Mode::Browse)),
        KeyCode::Enter => match e.to_app() {
            Ok(app) => {
                match e.editing_index {
                    Some(i) => list[i] = app,
                    None => {
                        list.push(app);
                        *selected = list.len() - 1;
                    }
                }
                *status = Some(match apps::save(list) {
                    Ok(()) => "已保存".to_string(),
                    Err(err) => format!("保存失败: {}", err),
                });
                return Ok(Next::Mode(Mode::Browse));
            }
            Err(msg) => e.error = Some(msg),
        },
        KeyCode::Tab | KeyCode::Down => e.cycle(true),
        KeyCode::BackTab | KeyCode::Up => e.cycle(false),
        KeyCode::Left => e.fields[e.focus].left(),
        KeyCode::Right => e.fields[e.focus].right(),
        KeyCode::Home => e.fields[e.focus].home(),
        KeyCode::End => e.fields[e.focus].end(),
        KeyCode::Backspace => {
            e.fields[e.focus].backspace();
            e.error = None;
        }
        KeyCode::Delete => {
            e.fields[e.focus].delete();
            e.error = None;
        }
        // 图标栏为空时，直接按 i 也可打开图标选择器
        KeyCode::Char('i')
            if e.focus == 0
                && e.fields[0].text().is_empty()
                && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            return Ok(Next::Mode(Mode::IconPicker(e, Picker::new())));
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            e.fields[e.focus].insert(c);
            e.error = None;
        }
        _ => {}
    }

    Ok(Next::Mode(Mode::Edit(e)))
}

/// 图标选择器的事件处理。
fn handle_picker(
    ev: Event,
    mut e: EditState,
    mut p: Picker,
    layout: &Layout,
) -> io::Result<Next> {
    let Event::Key(key) = ev else {
        return Ok(Next::Mode(Mode::IconPicker(e, p)));
    };
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Ok(Next::Mode(Mode::IconPicker(e, p)));
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(Next::Quit);
    }

    let cols = ui::picker_columns(layout);
    let rows = (layout.height as usize).saturating_sub(4).max(1);

    match key.code {
        KeyCode::Esc => return Ok(Next::Mode(Mode::Edit(e))),
        KeyCode::Enter | KeyCode::Char(' ') => {
            e.fields[0] = editor::Field::new(p.current());
            e.error = None;
            // 新增且名称还空着时，自动跳到名称栏，方便继续输入
            e.focus = if e.editing_index.is_none() && e.fields[1].text().is_empty() {
                1
            } else {
                0
            };
            return Ok(Next::Mode(Mode::Edit(e)));
        }
        KeyCode::Left | KeyCode::Char('h') => p.left(cols),
        KeyCode::Right | KeyCode::Char('l') => p.right(cols),
        KeyCode::Up | KeyCode::Char('k') => p.up(cols),
        KeyCode::Down | KeyCode::Char('j') => p.down(cols),
        KeyCode::PageUp => p.page(cols, rows, -1),
        KeyCode::PageDown => p.page(cols, rows, 1),
        KeyCode::Home => p.home(),
        KeyCode::End => p.end(),
        _ => {}
    }
    Ok(Next::Mode(Mode::IconPicker(e, p)))
}

/// 删除确认模式的事件处理。
fn handle_confirm(
    ev: Event,
    index: usize,
    list: &mut Vec<App>,
    selected: &mut usize,
    status: &mut Option<String>,
) -> io::Result<Next> {
    if let Event::Key(key) = ev
        && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(Next::Quit);
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if index < list.len() {
                    let removed = list.remove(index);
                    *selected = (*selected).min(list.len().saturating_sub(1));
                    *status = Some(match apps::save(list) {
                        Ok(()) => format!("已删除「{}」", removed.name),
                        Err(err) => format!("已删除，但保存失败: {}", err),
                    });
                }
                return Ok(Next::Mode(Mode::Browse));
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                return Ok(Next::Mode(Mode::Browse));
            }
            _ => {}
        }
    }
    Ok(Next::Mode(Mode::Confirm(index)))
}

/// 启动应用并返回一条状态提示（用于界面上显示）。
fn launch_status(name: &str, command: &str) -> String {
    match launcher::launch(name, command) {
        Ok(msg) => format!("{}「{}」", msg, name),
        Err(e) => format!("启动失败: {}", e),
    }
}

/// 安装 panic 钩子以恢复终端。
fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            Show,
            LeaveAlternateScreen,
            DisableMouseCapture,
            SetBackgroundColor(Color::Reset),
            SetForegroundColor(Color::Reset),
            SetAttribute(Attribute::Reset)
        );
        original(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn picker_enter_sets_selected_icon() {
        let layout = Layout::compute(80, 24);
        let e = EditState::new_app();
        let p = Picker::new();

        // 右移一格，选中第 2 个图标
        let Next::Mode(Mode::IconPicker(e, p)) =
            handle_picker(key(KeyCode::Right), e, p, &layout).unwrap()
        else {
            panic!("应保持在图标选择器")
        };
        assert_eq!(p.current(), icons::ICONS[1]);

        // 回车确认，回到编辑框并填入图标
        let Next::Mode(Mode::Edit(e)) =
            handle_picker(key(KeyCode::Enter), e, p, &layout).unwrap()
        else {
            panic!("应回到编辑模式")
        };
        assert_eq!(e.fields[0].text(), icons::ICONS[1]);
        // 新增且名称为空时自动跳到名称字段
        assert_eq!(e.focus, 1);
    }

    #[test]
    fn picker_esc_returns_without_changing_icon() {
        let layout = Layout::compute(80, 24);
        let e = EditState::new_app();
        let p = Picker::new();
        let Next::Mode(Mode::Edit(e)) =
            handle_picker(key(KeyCode::Esc), e, p, &layout).unwrap()
        else {
            panic!("应回到编辑模式")
        };
        assert_eq!(e.fields[0].text(), "");
    }
}

