# AppDrawer

手机 App 抽屉风格的交互式 TUI 启动器（Rust + crossterm），**跨平台**。
用图标网格管理常用命令，点击即在新窗口 / 新会话中启动，AppDrawer 本身保持运行。

## 平台支持

| 平台 | 启动方式 |
| --- | --- |
| Termux (Android) | 通过 `am startservice` 打开**新的 Termux 会话**（新窗口） |
| Windows | 默认用 **PowerShell** 执行：外部程序用 `Start-Process`（**无残留窗口**）；PowerShell 命令（cmdlet / 管道 / 变量等）在交互式 PowerShell 窗口中运行 |
| macOS | `open -a Terminal <脚本>`，在“终端”中打开 |
| Linux / 其它 Unix | 自动尝试常见终端模拟器：`x-terminal-emulator`、`gnome-terminal`、`kgx`、`konsole`、`xfce4-terminal`、`mate-terminal`、`tilix`、`terminator`、`kitty`、`alacritty`、`wezterm`、`foot`、`urxvt`、`xterm`、`st` |
| 兜底 | 以上都不可用时**后台分离运行**（不占用当前 TUI） |

窗口外观：默认是“手机抽屉”尺寸的紧凑窗口；Windows 上**双击即尝试真无边框**
（用 WezTerm / Alacritty 托管，没装就回退到 WT 紧凑窗口，再没有就地运行，见
[无边框方案完整调查表](#无边框方案完整调查表)）。

各平台的新窗口行为：

* **Termux**：通过 `am startservice` 打开新的 Termux 会话，命令结束后进入交互式 shell，窗口保留；
* **Linux / macOS**：在终端模拟器中运行，命令结束后进入交互式 shell，窗口保留；
* **Windows**：默认由 **PowerShell**（优先 `pwsh`，否则内置 `powershell`）执行。
  - 单个外部程序（如 `notepad`、`explorer .`、`ping 8.8.8.8 -t`）：用 `Start-Process` 启动，
    GUI 程序不产生额外窗口，控制台程序使用自己的窗口，**不留启动用的 cmd/PS 窗口**；
  - PowerShell 命令（如 `Get-Process | Sort-Object CPU`、`Get-Date`）：在新开的**交互式
    PowerShell 窗口**中运行，输出可见，并可继续输入命令。
  - 脚本以 UTF-8（含 BOM）+ `-ExecutionPolicy Bypass -NoProfile` 执行，避免中文乱码。

任意平台都可反复点击按钮启动多个实例，AppDrawer 本身始终运行。

### Windows 下的图标显示（旧版 cmd 显示不了 emoji）

**结论：旧版控制台 `conhost`（双击 exe 打开的那个黑框 cmd）根本无法渲染彩色 emoji。**

| 方案 | 能否在旧版 cmd 显示 emoji | 说明 |
| --- | --- | --- |
| 换字体（如 `Segoe UI Emoji`） | ❌ | conhost 要求等宽字体，且不支持字体回退/彩色 emoji |
| `SetCurrentConsoleFontEx` 强行设字体 | ❌ | 同上，最多拿到单色方块 |
| BMP 符号（`●◆★✓` 等） | ⚠️ 部分 | 依赖具体字体，不能保证 |
| **纯 ASCII 字符表情** | ✅ | 任何终端都能显示，这就是本项目的回退方案 |
| **Windows Terminal** | ✅ | 完整彩色 emoji（DirectWrite） |

因此 AppDrawer 在 Windows 上的行为是：

1. 把控制台输入/输出代码页切到 **UTF-8（65001）**，先避免中文乱码；
2. 自动检测是否处于**不支持 emoji 的终端**：若是，则改用**内置的 ASCII 字符表情**（如
   `:)` `:D` `;)` `:P` `<3` 等）渲染图标，并同步切换图标选择器里的可选图标；
3. 配置文件里的 emoji 图标**不会被修改**，只是在旧版 cmd 中被显示成对应的字符表情；
   在 Windows Terminal / Termux / Linux 里依旧显示原本的 emoji。

开关（环境变量）：

| 变量 | 作用 |
| --- | --- |
| `APPDRAWER_ASCII=1` / `=0` | 强制 ASCII 图标 / 强制 emoji |
| `APPDRAWER_THEME=light` / `=dark` | 配色主题（**默认亮色**） |
| `APPDRAWER_USE_WT=1` / `=0` | 开启 / 关闭“自动转到 Windows Terminal 中重新运行”（**默认开启**） |
| `APPDRAWER_NO_WT=1` | 只关掉“跳 Windows Terminal”这一步（无边框依旧会试） |
| `APPDRAWER_NO_RELAUNCH=1` | 完全不换窗口（连无边框也不试，总开关） |
| `APPDRAWER_WT_FULLSCREEN=1` | WT 新窗口用 `--fullscreen`（没有标题栏，占满屏幕） |
| `APPDRAWER_BORDERLESS=1` | 无边框（**Windows 上默认开启**，其它平台需手动开；`0` 关闭，`resize` = 保留可缩放的隐形边框） |
| `APPDRAWER_WINDOW_POS=x,y` | 指定窗口像素位置（默认按鼠标所在显示器**居中**） |
| `APPDRAWER_LOG=<路径>` | 指定日志文件路径（默认见下文） |
| `APPDRAWER_IN_WT=1` / `APPDRAWER_IN_BORDERLESS=1` | **内部用**：重启时自己给子进程打的标记，防止无限重启循环。不用手动设 |

> 启动日志默认写到临时目录：Windows 为 `%TEMP%\appdrawer.log`，Unix 为 `$TMPDIR/appdrawer.log`
> （可用 `APPDRAWER_LOG` 覆盖）。里面会记录是否跳转、每个 `wt.exe` 候选的启动结果、是否进入
> ASCII 模式等，排查“为什么还在旧 cmd 里”时直接看它即可。
>
> 第二行是环境快照，没设的变量记作 `-`，贴出来就能定位问题：
> `env: WT_SESSION=- … APPDRAWER_THEME="light" APPDRAWER_BORDERLESS=- …`

> 关于“自动跳转 Windows Terminal”的可靠性：代码**不靠** `wt.exe` 是否存在来判断，而是逐个候选
> **真正尝试启动**。因为 `%LOCALAPPDATA%\Microsoft\WindowsApps\wt.exe` 是“应用执行别名”
> （重解析点），`is_file()` / `exists()` 并不可靠；只有 `spawn()` 成功才算成功，失败就回退到
> 旧版控制台 + 字符表情。若 Windows 终端未安装或执行别名被禁用，`spawn()` 会失败，自动降级。
>
> 日志里会有这一行：`relaunch: 窗口尺寸 41x13`。若新窗口没弹出（例如你的 `wt.exe` 太旧、
> 不认 `--size`），设 `APPDRAWER_NO_WT=1` 就能回到“就近在当前终端里跑”的行为。
> 无边框模式下的日志长这样：`borderless: 已用 "…\wezterm.exe" 启动（pid=…，decorations=NONE）`，
> 找不到 WezTerm / Alacritty 时是 `borderless: 未找到 WezTerm / Alacritty，降级为带边框的窗口`。

> 想双击就直接看到 emoji，也可以把 Windows Terminal 设为默认终端：
> 设置 → 启动 → 默认终端应用程序 → Windows Terminal。

### 窗口大小与无边框

先给结论：**Windows Terminal 自己做不到无边框。** 它的命令行只有
`--maximized` / `--fullscreen` / `--size` / `--pos`，没有任何“去掉标题栏”
的参数；设置里也没有（`showTabsInTitlebar` 只是少画标签栏，标题栏还在）。

所以本项目的启动顺序是（**双击 exe 就是这个顺序，不需要设任何环境变量**）：

1. **真无边框**：换一个**本来就有** `decorations` 开关的终端来托管自己，
   依次尝试 **WezTerm** → **Alacritty**：
   * 先找**跟 `appdrawer.exe` 同一目录**的 `wezterm.exe`（免安装版解压到旁边即可）；
   * 再找 `PATH`；
   * 再找常见安装位置（`Program Files\WezTerm`、`%LOCALAPPDATA%\Programs\WezTerm`、
     Scoop、Chocolatey、Microsoft Store 的 `WindowsApps`）。
2. 都没有 → **Windows Terminal**：`wt.exe --size <列>,<行> <exe>` 开一个刚好装得下
   抽屉的紧凑窗口（7 个应用 → `41x13`）。想满屏无标题栏就用
   `APPDRAWER_WT_FULLSCREEN=1`（走 `--fullscreen`）。
3. 连 WT 都没有 / 启不来 → **就地在当前控制台窗口里跑**（带边框，不换窗口）。

“起来了但刚好挂了”（比如 WezTerm 某些版本不认某个参数）也能识别：
spawn 后观察 300 ms，**以失败码退出就换下一种写法**；七种写法
（无 `--always-new-process` / 无 `--` / 只设边框 / 值不带引号 / `--config` 放 start 后）
都不行才放弃换成 Alacritty，WezTerm 自己报的错会写进日志。

非 Windows 平台默认**不**自动换窗口（因为通常是你自己从终端里启动的），
想启用就显式设 `APPDRAWER_BORDERLESS=1`。

```bat
:: 双击默认就是无边框（装了 WezTerm / Alacritty 时）
appdrawer.exe

:: 无标题栏但保留可拖拽的缩放边框（更好操作）
set APPDRAWER_BORDERLESS=resize && appdrawer.exe

:: 不想要无边框（回到“WT 紧凑窗口 / 就地运行”的老行为）
set APPDRAWER_BORDERLESS=0 && appdrawer.exe

:: 手动钉一个位置（像素）；不设则自动居中
set APPDRAWER_WINDOW_POS=760,400 && appdrawer.exe
```

两种无边框终端实际执行的命令（日志里也能看到）：

```
:: --config 是【全局参数】，必须写在子命令 start 之前：
:: 写在 start 后面会被 clap 拒绝（unexpected argument '--config'）
wezterm --config window_decorations="NONE" \
        --config enable_tab_bar=false \
        --config initial_cols=41 --config initial_rows=13 \
        --config window_padding={left=0,right=0,top=0,bottom=0} \
        --config enable_scroll_bar=false \
        --config colors={background="#FFFEF8"} \
        start --position 763,395 --always-new-process -- C:\path\to\appdrawer.exe

alacritty --config-file %TEMP%\appdrawer-sessions\session-<pid>.toml
# 其中：[window] decorations = "none"，dimensions = { columns = 41, lines = 13 }
#       position = { x = 763, y = 395 }
#       [colors.primary] background = '#FFFEF8'
#       [terminal] shell = { program = "C:/path/to/appdrawer.exe" }
```

要关的东西是**四个分开的配置项**，少一样就留一样：

| 配置项 | 作用 |
| --- | --- |
| `window_decorations="NONE"` | 去掉系统标题栏 / 边框（`APPDRAWER_BORDERLESS=resize` 会给 `"RESIZE"`） |
| `enable_tab_bar=false` | 去掉顶部的标签栏（以及它右侧的滚动条） |
| `window_padding={…=0}` | 去掉窗口内边距（那圈是**终端**画的，不是我们） |
| `colors={background="#FFFEF8"}` | 把终端背景色改成界面底色（亮色主题时才传） |

#### 亮色界面周围那道黑边

亮色主题下，格子之外的那一圈**不是我们能画的地方**，它是终端用自己的背景色画的：

| 来源 | 表现 | 处理 |
| --- | --- | --- |
| `window_padding`（用户主题里设了） | 四边一圈均匀的黑框 | `--config window_padding={…=0}` |
| 窗口高 / 宽不是字格的整数倍（小数缩放常见） | 右边或底下一条 1～2 px 的黑线 | `--config colors={background=…}` |
| 右边滚动条 | 一条竖向色条 | `--config enable_scroll_bar=false` |

所以亮色主题下我们会把终端背景也改成 `#FFFEF8`；深色主题则不传 `colors`，
继续跟随你自己的配色（深色主题里底色本来就是 `Color::Reset`）。

参数不被接受时（`unexpected argument ...`）会自动换更保守的写法，共 8 种：
`--always-new-process` 去掉 → `--position` 去掉 → `--` 去掉 → 只设边框与标签栏
（内边距 / 滚动条 / 背景色一并丢掉，那是最后几级降级）→ 值不带引号 →
最后才是旧写法（`start --config …`）。

#### 窗口摆在屏幕正中

无边框窗口没有标题条，系统级的“拖住标题条移动”动作就不存在（窗口怎么拖见下一节）。
所以默认改成**启动就居中**：

1. 用 `GetCursorPos` + `MonitorFromPoint` 取**鼠标所在显示器的工作区**（不含任务栏，
   多屏也不会跑到另一块屏上）；
2. 按 `41 x 13` 字符估算窗口像素尺寸（WezTerm 默认字号 12pt，字宽 ≈ 0.6 em、行高 ≈ 1.2 em，
   再乘上 DPI 缩放）；
3. 算出居中坐标，作为 WezTerm 的 `--position X,Y` / Alacritty 的 `position = { x, y }`。

估算拿不到终端的真实字体度量（两家都没有查询接口），会有几像素偏差，但肯定是“屏幕中间”。
想钉死在某个位置：

```bat
set APPDRAWER_WINDOW_POS=760,400 && appdrawer.exe
```

登录后想要“每次都在同一处”，建议直接写进系统环境变量（`setx APPDRAWER_WINDOW_POS 760,400`）。

#### 拖动窗口

无边框窗口没有标题条，所以系统级的“拖标题条”动作不存在；WezTerm 自己的
`StartWindowDrag`（很多人会绑成 `Alt` + 左键）在 `window_decorations="NONE"` 的窗口上
同样不生效。所以 **AppDrawer 自己搬窗口**：

| 操作 | 结果 |
| --- | --- |
| **左键按住拖动**（窗口里任意处，移动超过 1 格） | 窗口跟着鼠标走（Windows） |
| **`Alt` + 左键拖动** | 同上（有的终端会先把 `Alt`+左键吃掉，所以不指望它） |
| **左键单击**（按下-抬起几乎没动） | 照旧启动应用 |
| `Win` + `←/→/↑` | 贴边 / 半屏 / 最大化（系统级命令，无边框也生效） |
| `Win` + `Shift` + `←/→` | 整个窗口搬到另一块显示器 |
| `set APPDRAWER_BORDERLESS=resize` | 换 `window_decorations="RESIZE"`：无标题条，但有**可拖拽的隐形缩放边框** |

实现要点：控制台进程的 `GetConsoleWindow()` 拿到的是 conhost 的隐藏窗口，不是屏幕上那个，
所以按**窗口标题**找（启动时 `SetTitle("AppDrawer")`，再用 `FindWindowW(nullptr, "AppDrawer")`）；
找不到就退一步用 `GetForegroundWindow()`，但要求它不属于本进程、且类名像终端
（`wezterm` / `alacritty` / `windowsterminal` / `cascadia` / `conhost` / `mintty`）；
两边都失败就安静地不启用——**宁可不拖，也绝不乱动别人的窗口**。
拖动时用 `SetWindowPos(SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)`，窗口不会突然跳到最前。

> 你在 `~/.wezterm.lua` 里绑的 `Alt` + 左键 `StartWindowDrag` 可以留着（对带边框的窗口有用），
> 但 AppDrawer 的窗口不靠它。

> 为什么不用 Win32 直接把标题栏砍掉？`SetWindowLongW(GWL_STYLE, style & !WS_CAPTION)`
> 对**旧版 conhost** 确实有效，但：① conhost 渲染不了彩色 emoji（只能退回本项目的
> ASCII 图标）；② Win11 默认终端是 WT 时，`GetConsoleWindow()` 拿到的是 WT 的
> `CASCADIA_HOSTING_WINDOW_CLASS` 窗口，改它的样式会连带弄坏标签栏，而且主题 / DPI
> 一变就被重置。所以没采用，只在下面的方案表里列出来。

窗口尺寸只影响“新开窗口”；改变终端大小后，界面会立即重排。

#### 排查：wezterm 在 PATH 里但没无边框窗口？

日志（`%TEMP%\appdrawer.log`）里应该有这几行：

```
borderless: 尝试开 41x13 无边框窗口，exe="...\\appdrawer.exe"
borderless: wezterm 候选 12 个，前几个：["C:\\Program Files\\WezTerm\\wezterm.exe", ...]
borderless: "...\\wezterm.exe" 写法 #1 启动后退出：exit status: 1：<wezterm 自己的错>
borderless: 已用 "...\\wezterm.exe" 启动（pid=1234，写法 #1，decorations=NONE）
```

逐行对应问题：

| 日志现象 | 含义 / 处理 |
| --- | --- |
| **一行 `borderless:` 都没有** | 这段根本没跑：看有没有 `relaunch: 跳过（APPDRAWER_NO_RELAUNCH…）`，或者已经被 `APPDRAWER_IN_BORDERLESS` 拦住（说明你其实就在里面） |
| `候选 N 个` 里没有你的路径 | 它没找到。把所在目录加进 `PATH`，或直接把 `wezterm.exe` 放 `appdrawer.exe` 旁边 |
| `写法 #1/#2 启动后退出：…` | wezterm **自己**报的错，接着会自动试更保守的写法 #2 ~ #7 |
| 没标题栏，但顶部还有一条“标签栏” | 那是 WezTerm 的 tab bar，**不归 `window_decorations` 管**，必须另给 `enable_tab_bar=false`（本版已默认加上；日志里没这个参数就重新编译） |
| 窗口四边还有一圈黑框 / 右边一条黑线 | 那是终端自己画的（内边距、取整像素、滚动条），现已默认传 `window_padding={…=0}` + `enable_scroll_bar=false` + `colors={background=…}`；日志里的参数只有 4 个 `--config` 就是旧二进制 |
| 错因是 `unexpected argument '--config'` | 旧版本不认 `start` 后的 `--config`（现已改为写在 `start` 之前，应在第一个写法就成功） |
| 错因是 `unexpected argument '--position'` | 版本不认 `--position`（居中失效），会自动换下一个不带位置的写法，窗口仍能开 |
| `所有写法都被拒绝，放弃 wezterm` | 版本差异太大，直接走 Alacritty / WT |
| `没有可用的无边框终端，降级为带边框的窗口` | 两个都没装 → 回退到 WT 紧凑窗口（再没有就就地运行） |

子进程的 stderr 全文存在 `%TEMP%\appdrawer-sessions\borderless-stderr.log`。

手动验证（在 cmd 里绕过 AppDrawer）：

```bat
:: 无标题栏也无标签栏、无黑边 → 说明参数没问题
wezterm --config window_decorations="NONE" --config enable_tab_bar=false ^
        --config window_padding={left=0,right=0,top=0,bottom=0} ^
        --config enable_scroll_bar=false ^
        --config colors={background="#FFFEF8"} start -- cmd
:: 如果这个也还有标题栏：先退出所有 WezTerm，再试；仍然有 → 版本太旧
```

### 无边框方案完整调查表

| # | 方案 | 平台 | 真无边框 | 代价 / 坑 | 本项目状态 |
| --- | --- | --- | --- | --- | --- |
| 1 | WT `--size` / `--pos` | Win | ❌ | — | ✅ 无边框装不了时的回退 |
| 2 | WT `--fullscreen`（`-F`） | Win | 近似（无标题栏，占满全屏） | 不能是“小窗口” | ✅ `APPDRAWER_WT_FULLSCREEN=1` |
| 3 | WT `launchMode: focus` / `showTabsInTitlebar: false` | Win | ❌ | 只是标题栏变窄 | — |
| 4 | **WezTerm `window_decorations="NONE"` + `enable_tab_bar=false`** | Win/mac/Linux | ✅ | 需要装 WezTerm（免安装 zip 即可） | ✅ 默认首选（Windows） |
| 5 | **Alacritty `decorations = "none"`** | Win/mac/Linux | ✅ | 需要装 Alacritty + 临时配置文件 | ✅ 默认次选（Windows） |
| 6 | kitty `hide_window_decorations yes` | Linux/mac | ✅ | 没有官方 Windows 版 | — |
| 7 | iTerm2（macOS）“Minimal” 主题 + 隐藏标签栏 | mac | 近似 | 只能这样，没有“无边框”开关 | — |
| 8 | Terminal.app（macOS） | mac | ❌ | 无此能力 | — |
| 9 | xterm `-bw 0` / urxvt `-b 0` | Linux | ✅（无边框，标题栏看 WM） | 字体/emoji 差，不能调尺寸与网格 | — |
| 10 | Win32 改 conhost 窗口样式 | Win | ✅ | 只能用 conhost → 没有彩色 emoji；WT 窗口改了会坏 | ❌ 有意不做 |
| 11 | 自己用 winit/softbuffer 画 GUI 窗口 | 全平台 | ✅（还能 `DwmSetWindowAttribute` 圆角） | 等于放弃终端：自己做字形缓存 / emoji 渲染 / 输入法 | ❌ 不是一个量级 |
| 12 | Termux 设置→全屏（+ 终端边距 0） | Android | 近似（只剩状态栏） | 要手动开一次 | 📄 见下 |
| 13 | adb `settings put global policy_control immersive.full=*` | Android | 近似（连状态栏也隐藏） | 需要一次 adb 授权 | 📄 见下 |

Termux / Android 上窗口完全由 Termux App 管理，程序内无法改：

* **Termux → 左侧抽屉 → Settings → Fullscreen**（隐藏工具栏和额外按键行）
* **Termux → 长按屏幕 → More → Style → 把 margin / padding 调到 0**
* 想连状态栏一起隐藏：`adb shell settings put global policy_control immersive.full=*`
  （撤销：`adb shell settings put global policy_control null`）

无边框窗口没有标题栏，记得怎么关：`Alt+F4`、`Alt+Space` → 关闭；
WezTerm 也可以用 `Ctrl+Shift+Q`。想还能拖边框缩放就用 `APPDRAWER_BORDERLESS=resize`。

### 双击启动时那个一闪而过的“cmd 窗口”

那是**真的**一个控制台窗口，不是我们画的，也不是无边框实现不干净。

`appdrawer.exe` 是**控制台子系统**程序（Rust 默认 `#![windows_subsystem = "console"]`）。
双击时 Windows 在**我们的代码執行之前**就先把控制台窗口建好并显示出来，
（Windows 11 默认终端是 Windows Terminal 时，还会先把整个 WT 拉起来，闪得更久）
紧接着它又被新开的无边框窗口取代——于是就有了那一下闪烁。

本项目的处理：`main` 第**一**行就把它藏起来（`ShowWindow(SW_HIDE)`），
同时用 `GetConsoleProcessList` 确认这个控制台是“为双击新建的”还是“从终端继承的”：

| 控制台里的进程 | 场景 | 做法 |
| --- | --- | --- |
| 只有本进程 | 双击 / 从 Explorer 启动 | `SW_HIDE` 立刻藏掉（日志：`console: 控制台只有本进程（双击启动）…`） |
| 还有 cmd.exe / fish / bash … | 在终端里运行 | **绝对不动**，那是用户自己的窗口 |

万一无边框 / WT 全都不成、只能就地运行，会把窗口 `SW_SHOW` 还回来（
日志：`console: 没能换到新窗口，已把控制台窗口还回来`），不会出现“程序在跑但看不见”的情况。

只能缩短、不能完全消除：窗口是 OS 在光速内建好的，我们只能尽早藏。
想彻底没白闪，两条路：

1. **建一个快捷方式**（推荐，零风险）：右键 `appdrawer.exe` → 发送到桌面快捷方式 →
   属性 → 运行方式：**最小化**。之后双击快捷方式，闪过的是任务栏一条，不是窗口。
2. 把 exe 改成 **GUI 子系统**（`#![windows_subsystem = "windows"]` + `AttachConsole`）：
   确实一点都不会闪，但“在已有终端 / 在 ConPTY 里运行”这条路径会变得依赖
   `AttachConsole` 的成功与否，而 crossterm 的尺寸 / 原生模式都依赖真控制台。
   为了一个几十毫秒的闪烁去赌主路径，不合算，所以没这么做。

## 构建与运行

```bash
cargo run --release
# 或
cargo build --release && ./target/release/appdrawer
```

仅依赖 `crossterm`，无需其它系统库。

## 代码地图

整个程序 12 个模块、约 5.6k 行（含测试）：

| 文件 | 行数 | 职责 |
| --- | --- | --- |
| `src/main.rs` | 651 | 入口、主循环、状态机（浏览 / 编辑 / 删除确认 / 图标选择）、鼠标与键盘事件 |
| `src/ui.rs` | 1102 | 只描述“这一帧长什么样”（网格布局、顶栏 / 底栏、弹窗、图标选择器） |
| `src/launcher.rs` | 1655 | 跨平台启动：Termux / Windows / macOS / Linux；无边框重启、居中、DPI |
| `src/winmove.rs` | 245 | Windows 无边框窗口拖动（找宿主 HWND + `SetWindowPos`） |
| `src/buffer.rs` | 601 | 终端虚拟缓冲（Cell 网格）、单元格 diff、转义序列输出 |
| `src/vdom.rs` | 305 | 虚拟 DOM：`Node` 树、按 key 协调的 diff |
| `src/icons.rs` | 353 | 图标集（198 个 emoji）+ ASCII 回退 + 图标选择器状态 |
| `src/theme.rs` | 221 | 亮色 / 深色两套配色，所有颜色集中在这里 |
| `src/apps.rs` | 174 | 配置读写（`图标\|名称\|命令`）、默认项、跨平台路径 |
| `src/editor.rs` | 176 | 添加 / 修改表单的字段状态与编辑操作 |
| `src/text.rs` | 114 | 显示宽度计算（CJK / emoji 记 2 列）、截断、补齐 |
| `src/log.rs` | 46 | 极简日志（互斥锁 + 512 KB 自动截断，失败静默） |

依赖方向是单向的：`main` → `ui` / `editor` / `launcher`，`ui` → `vdom` → `buffer` / `text`，
`launcher` 和 `theme` 不反向依赖界面。`winmove` 不改任何全局状态（除了自己记录的宿主窗口句柄），
在非 Windows 平台整个模块退化成空函数。

## 配置

配置文件每行一个应用，格式为 `图标|名称|命令`，`#` 开头为注释：

```text
# AppDrawer 配置：每行格式为  图标|名称|命令
🐚|Shell|bash
📁|Files|ls -la --color=auto
🌐|Net|curl -s ifconfig.me; echo
```

路径：

* Unix（含 Termux/Linux/macOS）：`$HOME/.config/appdrawer/apps.conf`
* Windows：`%APPDATA%\appdrawer\apps.conf`
* 可用环境变量 `APPDRAWER_CONFIG` 覆盖

首次运行会按平台写入一份默认配置。

## 界面布局（自适应手机抽屉）

网格不写死宽度，而是按终端尺寸自适应（`ui::grid_metrics`）：

| 终端宽度 | 列数 | 每格宽度 | 观感 |
| --- | --- | --- | --- |
| 20 | 2 | 9 | 极窄 / 分屏 |
| 41 | 4 | 9 | 手机竖屏 |
| 80 | 8 | 9 | 平板 / 桌面 |
| ≥200 | 8（封顶） | 变宽 | 不会把图标拉得太稀 |

* 每格只占 **2 行**（图标 + 名称），格子之间只留 **1 列**间距——比早期的
  12×3 大格子紧凑一倍多，80×24 能放下 80 个应用。
* 名称超出格宽时截断成 `一个很长…`，不会溢出到相邻图标。
* 选中项整格高亮（图标行 + 名称行共用一块背景），像手机桌面的选中态。
* 底栏按键提示自适应，优先级是“**先缩文案，再收紧间距，最后才丢弃**”：
  `←→↑↓ 选择   Enter 启动   a 添加 …` → `←→ 选择  Enter  a 加  e 改 …` →
  `←→ 选择 Enter a 加 e 改 d 删 r 刷 q 退`（38 列，默认 41 列窗口用的就是这个）。
  只要宽度 ≥ 38，**每个按键后面都带说明**（不会出现光杆 `r`），再窄才逐个舍去。

## 配色（亮色 / 深色）

默认**亮色**：整个窗口（网格空白、顶栏、弹窗）都由本项目自己铺底色，不依赖终端配色，
所以换终端、换主题，AppDrawer 都长得一样。想回到深色：

```bat
set APPDRAWER_THEME=dark && appdrawer.exe
:: 永久生效： setx APPDRAWER_THEME dark
```

| | 亮色（默认） | 深色 |
| --- | --- | --- |
| 窗口底色 | `#FFFEF8`（暖白） | 终端自己的背景（`Color::Reset`） |
| 顶栏 / 底栏 | 青绿 `#57C3C2` + 墨绿字 `#0F3A3A` | `#005A9E` + 白字 |
| 正文 / 次要文字 | `#202228` / `#6C707C` | 终端默认色 / 灰 |
| 选中格 | `#007ACC` + 白字 | `#0078D7` + 白字 |
| 弹窗底色 / 输入框 | `#FFFFFF` / `#EEF0F5` | `#181A22` / `#282C3A` |
| 命令预览 / 错误提示 | `#006E8C` / `#BE2828` | 深青 / 红 |

顶栏和底栏都是 `#57C3C2`。这个颜色很亮（相对亮度 0.45），白字压上去只有 2.1:1，
远低于 4.5:1 的可读下限，所以栏内文字用墨绿 `#0F3A3A`（约 5.9:1）——
`theme::tests::text_is_readable_on_every_surface` 会卡这条红线（所有前景 / 背景对
对比度不低于 4.5:1，正文类不低于 7:1）。

深色模式下底色仍取终端自己的背景，不会强行刷一层深色。

亮色主题下还会把**所在窗口的终端背景色**一并设成 `#FFFEF8`（无边框重启时作为
`colors={background=…}` 传给 WezTerm / `[colors.primary]` 传给 Alacritty）：
字符格子之外的边距、宽高取整剩下的几像素都属于“终端背景”，不一起改的话
就会在亮色界面周围留一道黑边。

## 快捷键

| 按键 | 功能 |
| --- | --- |
| `←→↑↓` / `hjkl` | 移动选择 |
| `Enter` / 鼠标单击 | 在新窗口启动 |
| 鼠标按住拖动 | 移动窗口（Windows，见「拖动窗口」） |
| `a` | 添加应用 |
| `e` | 修改应用 |
| `d` | 删除应用（`y`/`n` 确认） |
| `r` | 重新加载配置 |
| `q` / `Esc` / `Ctrl-C` | 退出 |

编辑表单内：`Tab`/`↑↓` 切换字段，`←→`/`Home`/`End`/`Backspace`/`Delete` 编辑，
`Ctrl+E`（图标栏为空时按 `i`）打开**内置表情选择器**，用方向键选择、`Enter` 确认，
`Enter` 保存，`Esc` 取消。

## 内置图标

添加 / 修改应用时，图标栏可直接从内置的常用表情里选（共 **198** 个宽 2 表情，按表情、自然、
设备、影音、文件、标记等分类），无需手动输入 emoji。选择器里已有的图标会高亮显示。

图标表里每个候选都是**显示宽度正好 2** 的单个 emoji（有单测卡这条），否则网格会错位。
ASCII 模式下换成另外 **34** 个纯 ASCII 字符表情（如 `:)` `:D` `;)` `:P` `<3`），
它们肯定是窄字符，不会把格子挤歪。

> 在旧版 Windows 控制台（conhost）中，选择器及网格会自动切换为一组纯 ASCII 字符表情
> （见上文“Windows 下的图标显示”），保证可显示；配置文件里的 emoji 不受影响。

## 渲染架构：终端虚拟 DOM

界面不是“每帧直接往 stdout 打印”，而是类似 React + [4at](https://github.com/tsoding/4at) 的两层结构：

```
ui::view()  ──►  vdom::Node 树（声明式描述这一帧）
                     │  按 key 协调（vdom::diff），完全一致则整帧跳过
                     ▼
              buffer::Buffer（终端虚拟缓冲，Cell 网格）
                     │  与上一帧做单元格 diff
                     ▼
              buffer::Renderer::present()  ──►  只写变化的转义序列
```

| 文件 | 职责 |
| --- | --- |
| `src/text.rs` | 显示宽度计算（CJK / emoji 记 2 列）、截断、补齐、居中 |
| `src/buffer.rs` | `Cell { ch, style, cont }` 网格、`Buffer`、`Patch`、`apply_patches`、双缓冲 `Renderer` |
| `src/vdom.rs` | `Node` 树、按 key 协调的 `diff()`、决定是否重画的 `VDom` |
| `src/ui.rs` | 只描述“这一帧长什么样”，返回 `Node` 树与输入光标位置 |

细节：

* 宽字符（中文 / emoji）占两格，右半格标记为 `cont`，diff 时会跳过，不会重复输出；
  用窄字符覆盖宽字符、或覆盖其右半格时，会自动清理另一半。emoji 一类“宽度不确定”
  的字符（`☀ ⚠ ❤` 等）会先把右半格补一个空格：某些终端 / 字体把它们画成 1 列，
  不补就会留下半个旧字符。
* 光标复用只信任“宽度确定”的字符（ASCII、制表符、中日韩全角）。箭头 / emoji 之后
  一律重新 `MoveTo`：否则一旦终端的实际宽度和估算不一致，整行会逐格向左漂移，
  弹窗右边框看起来就像“没闭合”。代价是每帧多几十字节，换来的是各终端下都对得齐。
* 弹窗里的文字左右各内缩 2 列，永远不会盖住边框；窄终端下提示文案会逐级退化成短版
  （`Enter 选择   Esc 取消` → `Enter / Esc`），而不是被截成半截。
* `apply_patches` 会复用连续的光标位置，只在样式变化时发送颜色 / 加粗序列。
* 关闭加粗用的是 `NormalIntensity`（SGR 22），**不是** `NoBold`（SGR 21）——
  21 在 ECMA-48 / Windows Terminal 里表示“双下划线”，会在文字下面留下双横线。
  每批补丁开头还会发一次 `SGR 0`，确保不会残留下划线 / 反色状态。
* 窗口尺寸变化（`Event::Resize`）会重建双缓冲并强制全量重绘。
* 界面代码因此可以“整屏重画”，终端只收到真正变化的单元格：实测 41×13 亮色首帧整体约 1.7 KB（含退出时的还原序列），之后每次移动选择只多约 **300 字节**（选中项是一个 2 行 × 9 列的高亮块，整块要重写）；整屏硬写则是 80×24 = 1920 个单元格、通常 10 KB 以上。

## 测试

```bash
cargo test                            # 62 个单元测试
cargo clippy --release --all-targets  # 0 警告
cargo build --release                 # 0 警告（无 dead_code / unused）
```

| 模块 | 用例 | 覆盖的核心不变式 |
| --- | --- | --- |
| `buffer` | 9 | 宽字符占两格但只输出一次；覆盖宽字符时清掉残留半格；diff 只报变化格；不会发出 SGR 21（双下划线）；非 ASCII 后游标重定位 |
| `ui` | 15 | 弹窗边框在任意尺寸（1×1 ~ 200×50）下都不被文字压住；图标选择器边框不破；亮色主题没有一格是 `(Reset, Reset)`；底栖永不出现“光秃秃的键名” |
| `launcher` | 16 | 无边框参数 8 种降级写法、`--config` 在 `start` 之前；窗口居中计算（96 / 144 DPI、跨屏、超屏）；脚本转义（`%` `,` `"`）；紧凑窗口尺寸 |
| `theme` | 4 | 环境变量解析；亮色为默认；所有前景 / 背景对对比度 ≥ 4.5:1（正文 ≥ 7:1） |
| `icons` | 6 | 198 个图标全是宽度 2；ASCII 回退确定性、全窄字符；选择器导航不越界 |
| `vdom` | 5 | 按 key 协调：相同树不重画；文本 / 样式 / 尺寸变化能正确反映 |
| `editor` | 3 | 编辑预填、字段编辑操作、名称 / 命令必填校验 |
| `main` / `text` | 2 + 2 | 状态切换；宽度计算与截断 |

> 它们都是**纯函数 / 纯结构**测试，不需要伪终端。
>
> 真正的终端行为（颜色、光标、鼠标、打字）另外用 `script(1)` 做伪终端回放验证：
> 把程序输出录下来，直接 grep 转义序列（例如亮色主题首帧里 `48;2;255;254;248` 出现 4 次、
> `48;2;87;195;194` 出现 2 次；鼠标点击 `\033[<0;3;3M` / `m` 后应该出现
> `已在新会话中启动「…」`）。
>
> Windows 专属的 Win32 调用（`FindWindowW` / `SetWindowPos` / `GetConsoleProcessList` …）
> 在 Linux 主机上无法执行，只能用临时 crate 把它们拆出来做类型检查；
> 所有失败路径都设计成**静默降级**（不拖动 / 不隐藏 / 不无边框），不会崩溃或动错窗口。
