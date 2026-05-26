# egui-editor

基于 [egui](https://github.com/emilk/egui) 的语法高亮代码查看器与查找栏组件。

提供可复用、主题参数化的代码查看/编辑体验，包含 tree-sitter 语法高亮引擎、全文搜索算法、浮动查找栏和大文本视窗渲染。

## 模块

| 模块 | 说明 |
|---|---|
| `highlight` | 基于 tree-sitter 的语法高亮引擎，支持 10 种语言（Java、Kotlin、XML、YAML、JSON、HTML、SQL、Bytecode、Properties、Plain），并补齐 Kotlin/JVM `$` 与反引号特殊方法名高亮 |
| `code_view` | 代码视图渲染 — 行号栏、语法高亮文本、搜索匹配绘制，支持只读和可编辑模式 |
| `search` | 搜索算法 — 纯文本 / 正则 / 全词匹配 / 大小写敏感，以及十六进制字节搜索；全词匹配会将 `$` 视作 JVM 名称字符 |
| `find_bar` | 浮动查找栏 overlay 组件，支持匹配导航和模式切换 |
| `theme` | 主题结构体（`SyntaxTheme`、`CodeViewTheme`、`FindBarTheme`），完整的配色 / 图标 / 标签参数化 |

## 用法示例（来自 Pervius）

### 定义主题

App 层通过工厂函数提供具体配色，crate 本身不硬编码任何颜色值：

```rust
// src/appearance/theme.rs
pub fn editor_theme() -> egui_editor::CodeViewTheme {
    egui_editor::CodeViewTheme {
        syntax: egui_editor::SyntaxTheme {
            text: SYN_TEXT,
            keyword: SYN_KEYWORD,
            string: SYN_STRING,
            // ...
        },
        bg: BG_DARKEST,
        gutter_bg: BG_GUTTER,
        line_number_color: TEXT_MUTED,
        search_bg: verdigris_alpha(25),
        search_current_bg: verdigris_alpha(60),
        code_font_size: 13.0,
    }
}
```

### 只读代码视图（反编译结果）

带行号映射（反编译器输出的原始行号可能不连续）和搜索匹配高亮：

```rust
// src/ui/editor/render.rs
pub fn render_decompiled(ui: &mut egui::Ui, tab: &mut EditorTab, matches: &[FindMatch], current: Option<usize>) {
    let t = theme::editor_theme();
    egui_editor::code_view::code_view(
        ui,
        &tab.decompiled,
        &tab.decompiled_data.spans,
        &tab.decompiled_line_mapping,
        matches,
        current,
        &t,
    );
}
```

### 可编辑代码视图

文本变更后自动触发语法高亮刷新，app 层处理副作用（标记 tab 为已修改）：

```rust
// src/ui/editor/render.rs
pub fn render_editable(ui: &mut egui::Ui, tab: &mut EditorTab, matches: &[FindMatch], current: Option<usize>) {
    let t = theme::editor_theme();
    let changed = egui_editor::code_view::code_view_editable(
        ui,
        &mut tab.decompiled,
        tab.language,
        matches,
        current,
        &t,
    );
    if changed {
        tab.refresh_decompiled_data();
        tab.is_modified = true;
    }
}
```

### 嵌入式代码视图（字节码面板）

结构化面板中嵌入 `code_view_editable`，与反编译视图共享行号、搜索高亮、边缘滚动等能力：

```rust
// src/ui/editor/bytecode_panel.rs — 方法字节码代码块
if method.has_code {
    let t = theme::editor_theme();
    changed |= egui_editor::code_view::code_view_editable(
        ui,
        &mut method.bytecode,
        egui_editor::Language::Bytecode,
        matches,
        current,
        &t,
    );
}
```

### 查找栏

`FindBar` 与业务逻辑解耦 — 通过 `update_text` / `update_bytes` 喂数据，支持文本和 hex 两种搜索模式：

```rust
// src/ui/editor/viewer.rs
// 根据当前视图模式喂入不同数据
if self.find_bar.open {
    match tab.active_view {
        ActiveView::Hex => self.find_bar.update_bytes(&tab.raw_bytes),
        ActiveView::Decompiled => self.find_bar.update_text(&tab.decompiled),
        ActiveView::Bytecode => self.find_bar.update_text(tab.selected_bytecode_text()),
    }
}

// 获取匹配结果，传递给 code_view 做高亮
let (matches, current) = if self.find_bar.open {
    self.find_bar.highlight_info()
} else {
    (vec![], None)
};

// 渲染浮动查找栏 overlay
if self.find_bar.open {
    let fbt = theme::find_bar_theme();
    self.find_bar.render_overlay(ui, content_rect, &fbt);
}
```

### 单独使用语法高亮引擎

不依赖 `code_view`，直接用 `highlight` 模块构建 `LayoutJob`（如搜索结果预览面板）：

```rust
// src/ui/search/tab.rs — 逐行高亮 + 匹配区间标记
let lang = if bytecode {
    highlight::Language::Bytecode
} else {
    highlight::Language::Java
};
let jobs = highlight::highlight_per_line(
    &sp.source_lines,
    lang,
    egui::FontId::monospace(11.0),
    sp.match_line,
    &line_ranges,
    MATCH_TEXT_BG,
    &theme::editor_theme().syntax,
);
```

## 依赖

- `eframe` 0.34.1
- `egui-shell`（兄弟 crate，提供 `FlatButton` 组件）
- `tree-sitter` 0.24 + 语言语法库

## 许可证

MIT
