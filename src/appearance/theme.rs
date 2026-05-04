//! 主题色常量
//!
//! 所有 hex 值标注在注释中。
//!
//! @author sky

use eframe::egui;
use rust_i18n::t;

use super::codicon;

/// 最深背景 #111112（Island 底色）
pub const BG_DARKEST: egui::Color32 = egui::Color32::from_rgb(17, 17, 18);
/// 行号栏背景 #131314（介于 island 底色与面板底色之间）
pub const BG_GUTTER: egui::Color32 = egui::Color32::from_rgb(19, 19, 20);
/// 主背景 #151516（窗口底色、Header、StatusBar）
pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(21, 21, 22);
/// 中层背景 #1C1C1E（输入框、编辑区、ViewToggle 容器）
pub const BG_MEDIUM: egui::Color32 = egui::Color32::from_rgb(28, 28, 30);
/// 浅层背景 #252527（关闭按钮 hover 等）
pub const BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(37, 37, 39);
/// 悬停背景 #2E2E31
pub const BG_HOVER: egui::Color32 = egui::Color32::from_rgb(46, 46, 49);

/// 主边框 #2E2E30
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(46, 46, 48);
/// 浅边框 #3A3A3D
pub const BORDER_LIGHT: egui::Color32 = egui::Color32::from_rgb(58, 58, 61);

/// 主要文字 #ECECEF
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(236, 236, 239);
/// 次要文字 #A0A0AB
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(160, 160, 171);
/// 暗淡文字 #5C5C6A
pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(92, 92, 106);

/// 主色调铜绿 #43B3AE
pub const VERDIGRIS: egui::Color32 = egui::Color32::from_rgb(67, 179, 174);
/// 辅助绿 #6EE7B7
pub const ACCENT_GREEN: egui::Color32 = egui::Color32::from_rgb(110, 231, 183);
/// 辅助橙 #E0A458
pub const ACCENT_ORANGE: egui::Color32 = egui::Color32::from_rgb(224, 164, 88);
/// 辅助红 #E06C75
pub const ACCENT_RED: egui::Color32 = egui::Color32::from_rgb(224, 108, 117);
/// 辅助青 #7EC8C8
pub const ACCENT_CYAN: egui::Color32 = egui::Color32::from_rgb(126, 200, 200);

/// 代码默认文字 #BCBEC4
pub const SYN_TEXT: egui::Color32 = egui::Color32::from_rgb(188, 190, 196);
/// 关键字 #CF8E6D
pub const SYN_KEYWORD: egui::Color32 = egui::Color32::from_rgb(207, 142, 109);
/// JVM 操作码 #6897BB（区别于关键字，偏蓝灰，JetBrains 风格）
pub const SYN_OPCODE: egui::Color32 = egui::Color32::from_rgb(104, 151, 187);
/// 字符串 #6AAB73
pub const SYN_STRING: egui::Color32 = egui::Color32::from_rgb(106, 171, 115);
/// 类型 / 类名（白色）
pub const SYN_TYPE: egui::Color32 = TEXT_PRIMARY;
/// 常量 / 字段 #C77DBB
pub const SYN_CONSTANT: egui::Color32 = egui::Color32::from_rgb(199, 125, 187);
/// 数字 #2AACB8
pub const SYN_NUMBER: egui::Color32 = egui::Color32::from_rgb(42, 172, 184);
/// 注释 #7A7E85
pub const SYN_COMMENT: egui::Color32 = egui::Color32::from_rgb(122, 126, 133);
/// 注解 #B3AE60
pub const SYN_ANNOTATION: egui::Color32 = egui::Color32::from_rgb(179, 174, 96);
/// 方法调用 #7CB4FF
pub const SYN_METHOD: egui::Color32 = egui::Color32::from_rgb(124, 180, 255);
/// 方法声明 #56A8F5
pub const SYN_METHOD_DECL: egui::Color32 = egui::Color32::from_rgb(86, 168, 245);

/// 标题栏按钮 hover #2A2A2F
pub const CAPTION_HOVER: egui::Color32 = egui::Color32::from_rgb(42, 42, 47);
/// 关闭按钮 hover #C42B1C（Win11 风格红）
pub const CLOSE_HOVER: egui::Color32 = egui::Color32::from_rgb(196, 43, 28);

/// Hex 视图 null 字节色 #3C3C4680（premultiplied）
pub const HEX_NULL: egui::Color32 = egui::Color32::from_rgba_premultiplied(60, 60, 70, 128);
/// Hex 视图行 hover 底色 #0F0F100F（premultiplied，极淡）
pub const HEX_ROW_HOVER: egui::Color32 = egui::Color32::from_rgba_premultiplied(15, 15, 16, 15);
/// Hex 视图字节 hover 底色 #112D2C40（premultiplied，铜绿色调）
pub const HEX_BYTE_HOVER: egui::Color32 = egui::Color32::from_rgba_premultiplied(17, 45, 44, 64);
/// Hex 视图选区底色 #1436344D（premultiplied，铜绿色调）
pub const HEX_SELECTION: egui::Color32 = egui::Color32::from_rgba_premultiplied(20, 54, 52, 77);
/// Hex 视图光标底色 #225A5780（premultiplied，铜绿色调）
pub const HEX_CURSOR: egui::Color32 = egui::Color32::from_rgba_premultiplied(34, 90, 87, 128);
/// Hex 视图列分隔线 #0D0D0D0D（premultiplied，近透明）
pub const HEX_SEPARATOR: egui::Color32 = egui::Color32::from_rgba_premultiplied(13, 13, 13, 13);

/// 标题栏高度
pub const TITLE_BAR_HEIGHT: f32 = 36.0;
/// 文件面板宽度
pub const FILE_PANEL_WIDTH: f32 = 260.0;
/// 状态栏高度
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// Island 圆角半径
pub const ISLAND_RADIUS: u8 = 8;
/// Island 之间的间距
pub const ISLAND_GAP: f32 = 6.0;
/// Island 到窗口左右边缘的水平边距
pub const ISLAND_MARGIN_H: f32 = 6.0;
/// Island 到窗口上下边缘的垂直边距
pub const ISLAND_MARGIN_V: f32 = 4.0;
/// 修改文件弹出列表单行高度
pub const MODIFIED_POPUP_ITEM_HEIGHT: f32 = 22.0;
/// 修改文件弹出列表内边距
pub const MODIFIED_POPUP_PAD: f32 = 4.0;
/// 修改文件弹出列表最大可见行数
pub const MODIFIED_POPUP_MAX_VISIBLE: usize = 12;
/// 修改文件弹出列表宽度
pub const MODIFIED_POPUP_WIDTH: f32 = 260.0;
/// 视图切换动画时长（秒）
pub const VIEW_TOGGLE_ANIM_DURATION: f32 = 0.15;
/// Explorer 面板最小宽度
pub const EXPLORER_MIN_WIDTH: f32 = 160.0;
/// Explorer 面板最大宽度
pub const EXPLORER_MAX_WIDTH: f32 = 600.0;
/// Explorer 折叠/展开动画时长（秒）
pub const EXPLORER_ANIM_DURATION: f32 = 0.08;
/// 搜索结果列表最小高度
pub const SEARCH_MIN_RESULTS_H: f32 = 80.0;
/// 搜索预览面板最小高度
pub const SEARCH_MIN_PREVIEW_H: f32 = 80.0;
/// 搜索面板分割线拖拽手柄高度
pub const SEARCH_RESIZE_HANDLE_H: f32 = 6.0;
/// 字节码详情内容区内边距
pub const BYTECODE_DETAIL_PAD: f32 = 16.0;
/// 字节码详情 key-value 键列宽度
pub const BYTECODE_KV_KEY_WIDTH: f32 = 100.0;
/// 文件树行高
pub const TREE_ROW_HEIGHT: f32 = 22.0;
/// 文件树展开/折叠动画时长（秒）
pub const TREE_EXPAND_DURATION: f32 = 0.12;
/// 字节码导航项行高
pub const BYTECODE_NAV_ROW_HEIGHT: f32 = 24.0;
/// 字节码 section 标签行高
pub const BYTECODE_SECTION_LABEL_HEIGHT: f32 = 28.0;
/// 字节码导航栏最小宽度
pub const BYTECODE_MIN_NAV_WIDTH: f32 = 120.0;
/// 字节码导航栏最大宽度
pub const BYTECODE_MAX_NAV_WIDTH: f32 = 500.0;
/// 字节码导航栏拖拽手柄宽度
pub const BYTECODE_RESIZE_HANDLE_W: f32 = 6.0;
/// 搜索结果行高
pub const SEARCH_ROW_HEIGHT: f32 = 24.0;
/// 搜索分组 header 行高
pub const SEARCH_GROUP_HEADER_HEIGHT: f32 = 28.0;

/// Explorer / Editor 面板共用的 island 样式
pub const ISLAND: egui_shell::components::IslandStyle = egui_shell::components::IslandStyle {
    radius: ISLAND_RADIUS as f32,
    fill: BG_DARKEST,
    mask: BG_DARK,
};

/// 铜绿色带透明度（用于选中高亮等）
pub fn verdigris_alpha(alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(
        (67u16 * alpha as u16 / 255) as u8,
        (179u16 * alpha as u16 / 255) as u8,
        (174u16 * alpha as u16 / 255) as u8,
        alpha,
    )
}

/// 扁平按钮主题
pub fn flat_button_theme(inactive_color: egui::Color32) -> egui_shell::components::FlatButtonTheme {
    egui_shell::components::FlatButtonTheme {
        text_primary: TEXT_PRIMARY,
        text_active: VERDIGRIS,
        text_inactive: inactive_color,
        bg_hover: BG_HOVER,
        bg_pressed: BG_LIGHT,
    }
}

/// 浮动窗口（Search / Settings 等）的 WindowConfig
pub fn window_config() -> egui_shell::WindowConfig {
    egui_shell::WindowConfig {
        frame: egui::Frame {
            fill: BG_GUTTER,
            corner_radius: egui::CornerRadius::same(8),
            stroke: egui::Stroke::new(1.0, BORDER),
            inner_margin: egui::Margin::same(0),
            shadow: egui::Shadow {
                spread: 2,
                blur: 20,
                offset: [0, 4],
                color: egui::Color32::from_black_alpha(80),
            },
            ..Default::default()
        },
        header_height: 32.0,
        pin_icon: codicon::PIN,
        pin_tooltip: t!("window.pin").to_string(),
        unpin_tooltip: t!("window.unpin").to_string(),
    }
}

/// 设置面板 widget 主题
pub fn settings_theme() -> egui_shell::components::SettingsTheme {
    egui_shell::components::SettingsTheme {
        text_primary: TEXT_PRIMARY,
        text_secondary: TEXT_SECONDARY,
        text_muted: TEXT_MUTED,
        bg_hover: BG_HOVER,
        bg_light: BG_LIGHT,
        bg_medium: BG_MEDIUM,
        bg_sidebar: BG_DARKEST,
        border: BORDER,
        accent: VERDIGRIS,
        icon_font: egui::FontFamily::Name("codicon".into()),
        chevron_icon: codicon::CHEVRON_DOWN,
    }
}

/// 菜单项主题
pub fn menu_theme() -> egui_shell::components::MenuTheme {
    egui_shell::components::MenuTheme {
        text_primary: TEXT_PRIMARY,
        text_muted: TEXT_MUTED,
        bg_hover: BG_HOVER,
    }
}

/// 状态栏主题
pub fn status_bar_theme() -> egui_shell::components::StatusBarTheme {
    egui_shell::components::StatusBarTheme {
        bg: BG_DARK,
        separator: BORDER,
    }
}

/// 确认弹窗主题
pub fn confirm_theme() -> egui_shell::components::ConfirmTheme {
    egui_shell::components::ConfirmTheme {
        frame: egui::Frame {
            fill: BG_LIGHT,
            stroke: egui::Stroke::new(1.0, BORDER_LIGHT),
            corner_radius: egui::CornerRadius::same(8),
            shadow: egui::Shadow {
                spread: 2,
                blur: 24,
                offset: [0, 8],
                color: egui::Color32::from_black_alpha(100),
            },
            ..Default::default()
        },
        title_color: TEXT_PRIMARY,
        message_color: TEXT_SECONDARY,
        separator: BORDER,
        backdrop: egui::Color32::from_black_alpha(80),
        button: flat_button_theme(TEXT_SECONDARY),
    }
}

/// 编辑器代码视图主题
pub fn editor_theme() -> egui_editor::CodeViewTheme {
    egui_editor::CodeViewTheme {
        syntax: egui_editor::SyntaxTheme {
            text: SYN_TEXT,
            keyword: SYN_KEYWORD,
            opcode: SYN_OPCODE,
            string: SYN_STRING,
            type_name: SYN_TYPE,
            number: SYN_NUMBER,
            comment: SYN_COMMENT,
            annotation: SYN_ANNOTATION,
            muted: TEXT_MUTED,
            constant: SYN_CONSTANT,
            method_call: SYN_METHOD,
            method_declaration: SYN_METHOD_DECL,
        },
        bg: BG_DARKEST,
        gutter_bg: BG_GUTTER,
        line_number_color: TEXT_MUTED,
        search_bg: verdigris_alpha(40),
        search_current_bg: verdigris_alpha(100),
        code_font_size: 13.0,
        line_highlight: verdigris_alpha(50),
        word_highlight_bg: verdigris_alpha(80),
    }
}

/// 编辑器查找栏主题
pub fn find_bar_theme() -> egui_editor::FindBarTheme {
    egui_editor::FindBarTheme {
        bg: BG_DARK,
        border: BORDER,
        text_primary: TEXT_PRIMARY,
        text_muted: TEXT_MUTED,
        error_color: ACCENT_RED,
        icons: egui_editor::theme::FindBarIcons {
            close: codicon::CLOSE,
            next: codicon::CHEVRON_DOWN,
            prev: codicon::CHEVRON_UP,
            regex: codicon::REGEX,
            whole_word: codicon::WHOLE_WORD,
            case_sensitive: codicon::CASE_SENSITIVE,
            search: codicon::SEARCH,
            font: codicon::family(),
        },
        labels: egui_editor::theme::FindBarLabels {
            close: t!("find.close").to_string(),
            next: t!("find.next").to_string(),
            prev: t!("find.prev").to_string(),
            use_regex: t!("find.use_regex").to_string(),
            match_word: t!("find.match_word").to_string(),
            match_case: t!("find.match_case").to_string(),
            no_results: t!("find.no_results").to_string(),
            result_fmt: |current, total| {
                t!("find.result_count", current = current, total = total).to_string()
            },
            hint: t!("find.hint").to_string(),
        },
        button: flat_button_theme(TEXT_MUTED),
    }
}
