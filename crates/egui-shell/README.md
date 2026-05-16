# egui-shell

基于 [egui](https://github.com/emilk/egui) 的跨平台窗口壳子，附带一套可复用 UI 组件。

Windows/Linux 使用 `decorations=false` 无框窗口 + 自绘标题栏（拖拽/最小化/最大化/关闭）+ 跨平台边缘 resize + Windows DWM 圆角；macOS 保留原生 traffic light 并适配标题栏留白。业务层只需实现 `AppContent` trait。

## 快速启动

```rust
use egui_shell::{run, AppContent, ShellOptions, ShellTheme};

struct MyApp { /* ... */ }

impl AppContent for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: &ShellTheme) {
        // 业务 UI
    }
    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        // 注入到标题栏左侧的菜单
    }
}

fn main() -> eframe::Result {
    let options = ShellOptions {
        title: "Pervius".to_owned(),
        size: [1280.0, 800.0],
        theme: ShellTheme { /* 配色 */ ..Default::default() },
    };
    run(options, |_cc| Box::new(MyApp { /* ... */ }))
}
```

壳子自动处理：
- SVG logo + 标题居中渲染
- codicon 图标字体加载
- 标题栏拖拽移动 / 双击最大化
- 窗口四边四角 resize 手柄
- Windows DWM 圆角（首帧自动启用）
- macOS traffic light 留白

## 组件

### FloatingWindow

带 pin 支持的主题化浮动窗口。基于 `egui::Area` 自绘，支持拖拽移动、四边 resize、pin/unpin、Escape 关闭、点击外部关闭：

```rust
use egui_shell::components::FloatingWindow;

let mut window = FloatingWindow::new("search", "Global Search")
    .icon("\u{eb51}")
    .default_size([700.0, 500.0])
    .min_size([400.0, 300.0]);

window.open();

// 每帧渲染
window.show(ctx, &shell_theme, |ui| { /* header 右侧按钮 */ }, |ui| {
    // 窗口内容
});
```

### FlatButton

扁平文本按钮，支持三态（idle / hover / active），统一用于菜单栏、搜索分类、模式切换等场景：

```rust
use egui_shell::components::{FlatButton, FlatButtonTheme};

let theme = FlatButtonTheme { /* 配色 */ };

// 普通按钮
ui.add(FlatButton::new("Decompile", &theme));

// 带 active 状态的切换按钮
ui.add(FlatButton::new("Bytecode", &theme).active(is_bytecode_view));

// 自定义图标字体
ui.add(FlatButton::new("\u{eb51}", &theme).font_family(icon_font).font_size(14.0));
```

### StatusBarWidget

通用状态栏容器，通过 `StatusItem` trait 注册项目，自动按 Left/Right 分组排列并插入分隔符：

```rust
use egui_shell::components::status_bar::{StatusBarWidget, StatusBarTheme, StatusItem, Alignment, ItemResponse};

struct ClassInfo { text: String }

impl StatusItem for ClassInfo {
    fn alignment(&self) -> Alignment { Alignment::Left }
    fn render(&mut self, ui: &mut egui::Ui, x: f32, center_y: f32) -> ItemResponse {
        // 在 (x, center_y) 处渲染文字，返回占用宽度
    }
}

let mut bar = StatusBarWidget::new(StatusBarTheme { bg: ..., separator: ... });
bar.add(ClassInfo { text: "MyClass.class".into() });
bar.render(ui);

// 按类型获取 item
if let Some(info) = bar.item_mut::<ClassInfo>() {
    info.text = "OtherClass.class".into();
}
```

### 菜单

`menu_item` / `menu_submenu` 函数，配合 `KeyBind` 显示快捷键标签，支持子菜单 hover 展开：

```rust
use egui_shell::components::{menu_item, menu_submenu, MenuTheme};
use egui_keybind::KeyBind;
use egui::Key;

const OPEN: KeyBind = KeyBind::ctrl(Key::O);
const SAVE: KeyBind = KeyBind::ctrl(Key::S);

let theme = MenuTheme { /* 配色 */ };

if menu_item(ui, &theme, "Open", Some(&OPEN)) { /* 打开文件 */ }
if menu_item(ui, &theme, "Save", Some(&SAVE)) { /* 保存 */ }

menu_submenu(ui, &theme, "Export", |ui| {
    if menu_item(ui, &theme, "As Java", None) { /* ... */ }
    if menu_item(ui, &theme, "As Bytecode", None) { /* ... */ }
});
```

### SettingsPanel

设置面板框架（侧栏 + 内容分栏），承载在 `FloatingWindow` 中。提供 `toggle`、`dropdown`、`slider`、`keybind_row`、`path_picker` 等 widget 原语，以及 `keybind_rows!` 宏批量渲染快捷键配置�行。

### SettingsFile

TOML 配置文件持久化 trait，提供 `load()` / `save()` 默认实现，路径基于 `dirs::config_dir()`。
