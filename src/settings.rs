//! 用户配置：数据定义 + 设置面板 UI
//!
//! TOML 持久化由 [`egui_shell::components::SettingsFile`] trait 提供，
//! 所有 section 均标注 `#[serde(default)]`，新增字段不会破坏旧配置文件。
//!
//! @author sky

use crate::appearance::{codicon, theme};
use eframe::egui;
use egui_keybind::KeyBind;
use egui_shell::components::{
    path_picker_with, section_header, toggle, FlatButton, SectionDef, SettingsFile, SettingsPanel,
    SettingsTheme,
};
use egui_shell::keybind_rows;
use pervius_java_bridge::decompiler::{self, CacheEntry};
use pervius_java_bridge::{environment, process};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 最近打开列表上限
const MAX_RECENT: usize = 10;

/// 界面语言
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh")]
    Zh,
    #[serde(rename = "en")]
    En,
}

impl Language {
    pub const ALL: &[Self] = &[Self::Zh, Self::En];

    /// 返回 rust-i18n 使用的 locale code
    pub fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    /// 返回语言显示名称
    pub fn label(self) -> String {
        match self {
            Self::Zh => t!("lang.zh").to_string(),
            Self::En => t!("lang.en").to_string(),
        }
    }
}

/// 打开第二个项目时的窗口策略
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenBehavior {
    /// 每次弹窗询问用户
    #[default]
    #[serde(rename = "ask")]
    Ask,
    /// 启动新进程实例打开
    #[serde(rename = "new_window")]
    NewWindow,
    /// 在当前窗口中替换现有项目
    #[serde(rename = "current_window")]
    CurrentWindow,
}

impl OpenBehavior {
    pub const ALL: &[Self] = &[Self::CurrentWindow, Self::NewWindow, Self::Ask];

    /// 返回显示名称
    pub fn label(self) -> String {
        match self {
            Self::CurrentWindow => t!("settings.open_behavior_current").to_string(),
            Self::NewWindow => t!("settings.open_behavior_new").to_string(),
            Self::Ask => t!("settings.open_behavior_ask").to_string(),
        }
    }
}

/// Kotlin 类反编译输出模式
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KotlinDecompilerKind {
    /// 输出 Kotlin 源码
    #[default]
    #[serde(rename = "vineflower")]
    Vineflower,
    /// 统一回退为 Java 源码
    #[serde(rename = "java")]
    Java,
}

impl KotlinDecompilerKind {
    pub const ALL: &[Self] = &[Self::Vineflower, Self::Java];

    pub fn label(self) -> String {
        match self {
            Self::Vineflower => t!("settings.kotlin_decompiler_vineflower").to_string(),
            Self::Java => t!("settings.kotlin_decompiler_java").to_string(),
        }
    }

    pub fn to_bridge(self) -> decompiler::KotlinDecompilerMode {
        match self {
            Self::Vineflower => decompiler::KotlinDecompilerMode::Vineflower,
            Self::Java => decompiler::KotlinDecompilerMode::Java,
        }
    }
}

/// 最近打开的文件条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentEntry {
    /// 文件完整路径
    pub path: String,
    /// 文件名（显示用）
    pub name: String,
    /// 打开时间（unix epoch 秒）
    pub timestamp: u64,
}

/// Java / 外部工具环境配置
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct JavaSettings {
    /// JAVA_HOME 路径（空字符串表示使用系统环境变量）
    pub java_home: String,
    /// Vineflower 版本
    pub vineflower_version: String,
    /// Vineflower 存储目录（空字符串表示使用默认缓存工具目录）
    pub vineflower_dir: String,
    /// Kotlin 版本
    pub kotlin_version: String,
    /// Kotlin 依赖存储目录（空字符串表示使用默认缓存工具目录）
    pub kotlin_dependencies_dir: String,
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            java_home: String::new(),
            vineflower_version: environment::DEFAULT_VINEFLOWER_VERSION.to_string(),
            vineflower_dir: String::new(),
            kotlin_version: environment::DEFAULT_KOTLIN_VERSION.to_string(),
            kotlin_dependencies_dir: String::new(),
        }
    }
}

impl JavaSettings {
    /// 转换为 bridge 层环境工具配置。
    pub fn environment_config(&self) -> environment::EnvironmentConfig {
        environment::EnvironmentConfig {
            vineflower_version: self.vineflower_version.clone(),
            vineflower_dir: path_option(&self.vineflower_dir),
            kotlin_version: self.kotlin_version.clone(),
            kotlin_dependencies_dir: path_option(&self.kotlin_dependencies_dir),
        }
    }
}

fn path_option(path: &str) -> Option<std::path::PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.into())
    }
}

fn pick_folder_string() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

/// 编译配置
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileSettings {
    /// Kotlin 类反编译输出模式
    pub kotlin_decompiler: KotlinDecompilerKind,
    /// Kotlin 编译时跳过依赖元数据版本检查
    pub kotlin_skip_metadata_version_check: bool,
    /// 全局编译 classpath 条目（JAR / ZIP / 目录）。
    pub classpath_entries: Vec<String>,
}

impl Default for CompileSettings {
    fn default() -> Self {
        Self {
            kotlin_decompiler: KotlinDecompilerKind::Vineflower,
            kotlin_skip_metadata_version_check: true,
            classpath_entries: Vec::new(),
        }
    }
}

/// 反编译缓存配置
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheSettings {
    /// 自定义反编译缓存目录（空字符串表示使用系统默认目录）
    pub decompiled_dir: String,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            decompiled_dir: String::new(),
        }
    }
}

impl CacheSettings {
    /// 返回用户配置的缓存根目录
    pub fn root_path(&self) -> Option<&Path> {
        let trimmed = self.decompiled_dir.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Path::new(trimmed))
        }
    }
}

/// 快捷键配置
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapSettings {
    /// 切换资源管理器
    pub toggle_explorer: KeyBind,
    /// 打开 JAR
    pub open_jar: KeyBind,
    /// 打开查找
    pub find: KeyBind,
    /// 打开文件中查找
    pub find_in_files: KeyBind,
    /// 保存当前标签页
    pub save: KeyBind,
    /// 关闭当前标签页
    pub close_tab: KeyBind,
    /// 关闭全部标签页
    pub close_all_tabs: KeyBind,
    /// 导出反编译结果
    pub export_decompiled: KeyBind,
    /// 导出 JAR
    pub export_jar: KeyBind,
    /// 循环切换视图
    pub cycle_view: KeyBind,
    /// 打开设置
    pub open_settings: KeyBind,
    /// 切换视窗模式
    pub toggle_viewport: KeyBind,
}

impl Default for KeymapSettings {
    fn default() -> Self {
        use crate::ui::keybindings;
        Self {
            toggle_explorer: keybindings::DEFAULT_TOGGLE_EXPLORER,
            open_jar: keybindings::DEFAULT_OPEN_JAR,
            find: keybindings::DEFAULT_FIND,
            find_in_files: keybindings::DEFAULT_FIND_IN_FILES,
            save: keybindings::DEFAULT_SAVE,
            close_tab: keybindings::DEFAULT_CLOSE_TAB,
            close_all_tabs: keybindings::DEFAULT_CLOSE_ALL_TABS,
            export_decompiled: keybindings::DEFAULT_EXPORT_DECOMPILED,
            export_jar: keybindings::DEFAULT_EXPORT_JAR,
            cycle_view: keybindings::DEFAULT_CYCLE_VIEW,
            open_settings: keybindings::DEFAULT_OPEN_SETTINGS,
            toggle_viewport: keybindings::DEFAULT_TOGGLE_VIEWPORT,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            open_behavior: OpenBehavior::default(),
            java: JavaSettings::default(),
            compile: CompileSettings::default(),
            cache: CacheSettings::default(),
            keymap: KeymapSettings::default(),
            recent: Vec::new(),
        }
    }
}

impl SettingsFile for Settings {
    fn app_name() -> &'static str {
        "pervius"
    }
}

tabookit::class! {
    /// 顶层配置
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct Settings {
        /// 界面语言
        pub language: Language,
        /// 打开第二个项目时的窗口策略
        pub open_behavior: OpenBehavior,
        /// Java 环境设置
        pub java: JavaSettings,
        /// 编译设置
        pub compile: CompileSettings,
        /// 反编译缓存设置
        pub cache: CacheSettings,
        /// 快捷键设置
        pub keymap: KeymapSettings,
        /// 最近打开列表
        pub recent: Vec<RecentEntry>,
    }

    /// 仅读取语言配置（用于启动时在 UI 初始化前设置 locale）
    pub fn load_for_locale() -> Self {
        Self::load()
    }

    /// 将文件加入最近打开列表头部（去重 + 截断）
    pub fn add_recent(&mut self, path: &Path, name: &str) {
        let path_str = path.to_string_lossy().into_owned();
        self.recent.retain(|e| e.path != path_str);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.recent.insert(
            0,
            RecentEntry {
                path: path_str,
                name: name.to_owned(),
                timestamp,
            },
        );
        self.recent.truncate(MAX_RECENT);
    }

    /// 从最近打开列表中移除指定路径
    pub fn remove_recent(&mut self, path: &Path) {
        let path_str = path.to_string_lossy();
        self.recent.retain(|e| e.path != *path_str);
    }

    /// 清空最近打开列表
    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

/// 设置面板触发的非持久化动作
#[derive(Clone, Debug)]
pub enum SettingsAction {
    /// 删除指定缓存
    DeleteCache {
        /// 缓存目录
        dir: std::path::PathBuf,
        /// 展示名称
        label: String,
    },
    /// 删除当前缓存目录下的全部缓存
    DeleteAllCaches {
        /// 当前缓存条数
        count: usize,
    },
}

/// 设置面板的缓存状态
#[derive(Default)]
pub struct SettingsPanelState {
    /// 当前缓存列表
    pub cache_entries: Vec<CacheEntry>,
    /// 缓存列表加载失败信息
    pub cache_error: Option<String>,
    /// 缓存操作是否进行中
    pub cache_busy: bool,
}

/// 设置面板输出
pub struct SettingsOutput {
    /// 配置变更
    pub settings: Option<Settings>,
    /// 一次性动作
    pub action: Option<SettingsAction>,
}

/// 构造预配置的设置面板实例
pub fn new_panel() -> SettingsPanel<Settings> {
    SettingsPanel::new("settings_window", t!("settings.title").to_string())
        .icon(codicon::SETTINGS_GEAR)
        .default_size([700.0, 500.0])
        .min_size([520.0, 380.0])
}

/// 刷新缓存列表状态
pub fn refresh_cache_state(state: &mut SettingsPanelState) {
    match decompiler::list_cache_entries() {
        Ok(entries) => {
            state.cache_entries = entries;
            state.cache_error = None;
        }
        Err(error) => {
            state.cache_entries.clear();
            state.cache_error = Some(error.to_string());
        }
    }
}

/// 渲染设置面板
pub fn show(
    panel: &mut SettingsPanel<Settings>,
    state: &mut SettingsPanelState,
    ctx: &egui::Context,
    shell_theme: &egui_shell::ShellTheme,
) -> SettingsOutput {
    let st = theme::settings_theme();
    let sections = [
        SectionDef {
            icon: codicon::SETTINGS_GEAR,
            label: t!("settings.general").to_string(),
        },
        SectionDef {
            icon: codicon::BEAKER,
            label: t!("settings.environment").to_string(),
        },
        SectionDef {
            icon: codicon::TOOLS,
            label: t!("settings.compile").to_string(),
        },
        SectionDef {
            icon: codicon::FOLDER,
            label: t!("settings.cache").to_string(),
        },
        SectionDef {
            icon: codicon::KEYBOARD,
            label: t!("settings.keymap").to_string(),
        },
    ];
    let settings = panel.show(ctx, shell_theme, &st, &sections, |draft, active, ui, st| {
        render_section(draft, active, ui, st, state)
    });
    let action = take_settings_action(ctx);
    SettingsOutput { settings, action }
}

/// 渲染指定 section 的内容
fn render_section(
    draft: &mut Settings,
    active: usize,
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    state: &mut SettingsPanelState,
) -> bool {
    match active {
        0 => render_general(draft, ui, st),
        1 => render_java(draft, ui, st),
        2 => render_compile(draft, ui, st),
        3 => render_cache(draft, ui, st, state),
        _ => render_keymap(&mut draft.keymap, ui, st),
    }
}

fn render_general(draft: &mut Settings, ui: &mut egui::Ui, st: &SettingsTheme) -> bool {
    let mut changed = false;
    section_header(ui, st, &t!("settings.section_general"));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("settings.language").to_string())
                .size(13.0)
                .color(st.text_primary),
        );
        ui.add_space(8.0);
        let current = draft.language;
        egui::ComboBox::from_id_salt("language_combo")
            .selected_text(current.label())
            .width(120.0)
            .show_ui(ui, |ui| {
                for &lang in Language::ALL {
                    if ui.selectable_label(current == lang, lang.label()).clicked() {
                        draft.language = lang;
                        rust_i18n::set_locale(lang.code());
                        changed = true;
                    }
                }
            });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("settings.open_behavior").to_string())
                .size(13.0)
                .color(st.text_primary),
        );
        ui.add_space(8.0);
        let current = draft.open_behavior;
        egui::ComboBox::from_id_salt("open_behavior_combo")
            .selected_text(current.label())
            .width(140.0)
            .show_ui(ui, |ui| {
                for &behavior in OpenBehavior::ALL {
                    if ui
                        .selectable_label(current == behavior, behavior.label())
                        .clicked()
                    {
                        draft.open_behavior = behavior;
                        changed = true;
                    }
                }
            });
    });
    ui.add_space(4.0);
    changed
}

fn render_java(draft: &mut Settings, ui: &mut egui::Ui, st: &SettingsTheme) -> bool {
    let mut changed = false;
    section_header(ui, st, &t!("settings.environment"));
    changed |= path_picker_with(
        ui,
        st,
        &t!("settings.java_home"),
        &mut draft.java.java_home,
        &t!("settings.java_home_hint"),
        &t!("settings.browse"),
        pick_folder_string,
    );
    paint_java_path_hint(ui, st, &draft.java.java_home);
    ui.add_space(10.0);
    section_header(ui, st, &t!("settings.vineflower_tools"));
    changed |= render_text_field_row(
        ui,
        st,
        &t!("settings.vineflower_version"),
        &mut draft.java.vineflower_version,
        None,
    );
    changed |= path_picker_with(
        ui,
        st,
        &t!("settings.vineflower_dir"),
        &mut draft.java.vineflower_dir,
        &t!("settings.vineflower_dir_hint"),
        &t!("settings.browse"),
        pick_folder_string,
    );
    paint_tool_dir_hint(ui, st, effective_vineflower_dir(draft));
    ui.add_space(10.0);
    section_header(ui, st, &t!("settings.kotlin_tools"));
    changed |= render_text_field_row(
        ui,
        st,
        &t!("settings.kotlin_version"),
        &mut draft.java.kotlin_version,
        None,
    );
    changed |= path_picker_with(
        ui,
        st,
        &t!("settings.kotlin_dependencies_dir"),
        &mut draft.java.kotlin_dependencies_dir,
        &t!("settings.kotlin_dependencies_dir_hint"),
        &t!("settings.browse"),
        pick_folder_string,
    );
    paint_tool_dir_hint(ui, st, effective_kotlin_dependencies_dir(draft));
    changed
}

fn effective_tool_dir(
    draft: &Settings,
    configured: &str,
    sub_dir: &str,
) -> Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError> {
    path_option(configured)
        .map(Ok)
        .unwrap_or_else(|| Ok(effective_dependencies_root(draft)?.join(sub_dir)))
}

fn effective_vineflower_dir(
    draft: &Settings,
) -> Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError> {
    effective_tool_dir(draft, &draft.java.vineflower_dir, "vineflower")
}

fn effective_kotlin_dependencies_dir(
    draft: &Settings,
) -> Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError> {
    effective_tool_dir(draft, &draft.java.kotlin_dependencies_dir, "kotlin")
}

fn effective_dependencies_root(
    draft: &Settings,
) -> Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError> {
    let cache_root = effective_cache_root(draft)?;
    Ok(cache_root
        .parent()
        .map(|parent| parent.join("dependencies"))
        .unwrap_or_else(|| cache_root.join("dependencies")))
}

fn effective_cache_root(
    draft: &Settings,
) -> Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError> {
    if let Some(path) = draft.cache.root_path() {
        return Ok(path.to_path_buf());
    }
    let base = dirs::cache_dir().ok_or(pervius_java_bridge::error::BridgeError::NoCacheDir)?;
    Ok(base.join("pervius").join("decompiled"))
}

fn render_text_field_row(
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    label: &str,
    value: &mut String,
    hint: Option<&str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(label.to_string())
                .size(13.0)
                .color(st.text_primary),
        );
        ui.add_space(8.0);
        let mut edit = egui::TextEdit::singleline(value).desired_width(160.0);
        if let Some(hint) = hint {
            edit = edit.hint_text(hint);
        }
        let resp = ui.add(edit);
        changed |= resp.changed();
    });
    ui.add_space(4.0);
    changed
}

fn paint_java_path_hint(ui: &mut egui::Ui, st: &SettingsTheme, configured: &str) {
    let text = match process::resolve_java_path(configured) {
        Ok(path) => t!("settings.java_current_path", path = path.display()).to_string(),
        Err(error) => t!("settings.java_current_path_failed", error = error.to_string()).to_string(),
    };
    paint_path_hint_line(ui, st, text);
}

fn paint_tool_dir_hint(
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    path: Result<std::path::PathBuf, pervius_java_bridge::error::BridgeError>,
) {
    let text = match path {
        Ok(path) => t!("settings.tool_current_dir", path = path.display()).to_string(),
        Err(error) => t!("settings.tool_current_dir_failed", error = error.to_string()).to_string(),
    };
    paint_path_hint_line(ui, st, text);
}

fn paint_section_hint(ui: &mut egui::Ui, st: &SettingsTheme, text: String) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(text)
                .size(11.0)
                .color(st.text_secondary),
        );
    });
}

fn paint_path_hint_line(ui: &mut egui::Ui, st: &SettingsTheme, text: String) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(text)
                .size(11.0)
                .color(st.text_secondary)
                .monospace(),
        );
    });
}

fn render_compile(draft: &mut Settings, ui: &mut egui::Ui, st: &SettingsTheme) -> bool {
    let mut changed = false;
    section_header(ui, st, &t!("settings.section_compile"));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("settings.kotlin_decompiler").to_string())
                .size(13.0)
                .color(st.text_primary),
        );
        ui.add_space(8.0);
        let current = draft.compile.kotlin_decompiler;
        egui::ComboBox::from_id_salt("kotlin_decompiler_combo")
            .selected_text(current.label())
            .width(180.0)
            .show_ui(ui, |ui| {
                for &mode in KotlinDecompilerKind::ALL {
                    if ui.selectable_label(current == mode, mode.label()).clicked() {
                        draft.compile.kotlin_decompiler = mode;
                        changed = true;
                    }
                }
            });
    });
    paint_section_hint(
        ui,
        st,
        t!("settings.kotlin_decompiler_hint").to_string(),
    );
    ui.add_space(8.0);
    changed |= toggle(
        ui,
        st,
        &t!("settings.kotlin_skip_metadata_version_check"),
        &mut draft.compile.kotlin_skip_metadata_version_check,
    );
    paint_section_hint(
        ui,
        st,
        t!("settings.kotlin_skip_metadata_version_check_hint").to_string(),
    );
    ui.add_space(10.0);
    section_header(ui, st, &t!("settings.compile_classpath"));
    paint_compile_classpath_hint(ui, st);
    changed |= render_classpath_actions(ui, st, &mut draft.compile.classpath_entries);
    changed |= render_classpath_entries(ui, st, &mut draft.compile.classpath_entries);
    changed
}

fn paint_compile_classpath_hint(ui: &mut egui::Ui, st: &SettingsTheme) {
    paint_section_hint(ui, st, t!("settings.compile_classpath_hint").to_string());
}

fn render_classpath_actions(
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    entries: &mut Vec<String>,
) -> bool {
    let mut changed = false;
    let fbt = theme::flat_button_theme(theme::TEXT_SECONDARY);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        if ui
            .add(
                FlatButton::new(&t!("settings.classpath_add_jar"), &fbt)
                    .font_size(11.5)
                    .min_size(egui::vec2(0.0, 22.0)),
            )
            .clicked()
        {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter(&*t!("layout.java_archive"), &["jar", "zip", "war", "ear"])
                .pick_files()
            {
                changed |= add_classpath_entries(entries, paths);
            }
        }
        if ui
            .add(
                FlatButton::new(&t!("settings.classpath_add_dir"), &fbt)
                    .font_size(11.5)
                    .min_size(egui::vec2(0.0, 22.0)),
            )
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                changed |= add_classpath_entries(entries, [path]);
            }
        }
        ui.label(
            egui::RichText::new(t!("settings.classpath_count", count = entries.len()).to_string())
                .size(11.0)
                .color(st.text_secondary),
        );
    });
    changed
}

fn add_classpath_entries<I, P>(entries: &mut Vec<String>, paths: I) -> bool
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut changed = false;
    for path in paths {
        let path = path.as_ref().to_string_lossy().into_owned();
        if !path.trim().is_empty() && !entries.iter().any(|p| p == &path) {
            entries.push(path);
            changed = true;
        }
    }
    changed
}

fn elide_middle(ui: &egui::Ui, text: &str, font: egui::FontId, max_width: f32) -> String {
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::WHITE)
            .rect
            .width()
            <= max_width
    };
    if max_width <= 0.0 || fits(text) {
        return text.to_owned();
    }
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= 3 {
        return "…".to_owned();
    }
    let mut keep = chars.len().saturating_sub(1);
    while keep > 1 {
        let head = keep / 2;
        let tail = keep - head;
        let candidate = format!(
            "{}…{}",
            chars[..head].iter().collect::<String>(),
            chars[chars.len() - tail..].iter().collect::<String>()
        );
        if fits(&candidate) {
            return candidate;
        }
        keep -= 1;
    }
    "…".to_owned()
}

fn render_classpath_entries(
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    entries: &mut Vec<String>,
) -> bool {
    let mut changed = false;
    let fbt = theme::flat_button_theme(theme::TEXT_SECONDARY);
    let mut remove = None;
    if entries.is_empty() {
        paint_cache_message(ui, st, &t!("settings.classpath_empty").to_string());
        return false;
    }
    for (idx, entry) in entries.iter().enumerate() {
        let avail_w = ui.available_width();
        let row_height = 30.0;
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(avail_w, row_height), egui::Sense::hover());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, st.bg_hover);
        }
        let exists = Path::new(entry).exists();
        let color = if exists { st.text_primary } else { st.text_muted };
        let left = rect.left() + 16.0;
        let mid_y = rect.center().y;
        let btn_w = 22.0;
        let btn_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0 - btn_w * 0.5, mid_y),
            egui::vec2(btn_w, btn_w),
        );
        let missing_w = if exists { 0.0 } else { 48.0 };
        let text_right = (btn_rect.left() - 8.0 - missing_w).max(left + 24.0);
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(text_right, rect.bottom()),
        );
        let display_entry = elide_middle(
            ui,
            entry,
            egui::FontId::monospace(11.0),
            text_rect.width(),
        );
        ui.painter().text(
            egui::pos2(left, mid_y),
            egui::Align2::LEFT_CENTER,
            display_entry,
            egui::FontId::monospace(11.0),
            color,
        );
        if !exists {
            ui.painter().text(
                egui::pos2(btn_rect.left() - 8.0, mid_y),
                egui::Align2::RIGHT_CENTER,
                t!("settings.classpath_missing").to_string(),
                egui::FontId::proportional(10.5),
                st.text_muted,
            );
        }
        let mut btn_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(btn_rect)
                .id_salt(egui::Id::new("classpath_del").with(idx)),
        );
        if btn_ui
            .add(
                FlatButton::new(codicon::CLOSE, &fbt)
                    .font_size(12.0)
                    .font_family(codicon::family())
                    .min_size(egui::vec2(btn_w, btn_w)),
            )
            .clicked()
        {
            remove = Some(idx);
        }
    }
    if let Some(idx) = remove {
        entries.remove(idx);
        changed = true;
    }
    changed
}

fn render_cache(
    draft: &mut Settings,
    ui: &mut egui::Ui,
    st: &SettingsTheme,
    state: &mut SettingsPanelState,
) -> bool {
    let mut changed = false;
    section_header(ui, st, &t!("settings.section_cache"));
    changed |= path_picker_with(
        ui,
        st,
        &t!("settings.cache_dir"),
        &mut draft.cache.decompiled_dir,
        &t!("settings.cache_dir_hint"),
        &t!("settings.browse"),
        pick_folder_string,
    );
    ui.add_space(6.0);
    paint_cache_root_hint(ui, st);
    ui.add_space(10.0);
    section_header(ui, st, &t!("settings.cache_entries"));
    ui.add_space(6.0);
    paint_cache_actions(ui, st, state);
    ui.add_space(6.0);
    if let Some(error) = &state.cache_error {
        paint_cache_message(
            ui,
            st,
            &t!("settings.cache_list_failed", error = error).to_string(),
        );
        return false;
    }
    if state.cache_entries.is_empty() {
        paint_cache_message(ui, st, &t!("settings.cache_empty").to_string());
        return false;
    }
    for entry in &state.cache_entries {
        paint_cache_entry(ui, st, entry, state.cache_busy);
    }
    changed
}

fn render_keymap(km: &mut KeymapSettings, ui: &mut egui::Ui, st: &SettingsTheme) -> bool {
    let defaults = KeymapSettings::default();
    let hint = t!("settings.press_key");
    let mut changed = false;
    section_header(ui, st, &t!("settings.section_general"));
    changed |= keybind_rows!(ui, st, hint, km, defaults,
        t!("settings.open_jar") => open_jar,
        t!("settings.toggle_explorer") => toggle_explorer,
        t!("settings.open_settings") => open_settings,
    );
    section_header(ui, st, &t!("settings.section_editor"));
    changed |= keybind_rows!(ui, st, hint, km, defaults,
        t!("settings.save") => save,
        t!("settings.find") => find,
        t!("settings.find_in_files") => find_in_files,
        t!("settings.close_tab") => close_tab,
        t!("settings.close_all_tabs") => close_all_tabs,
        t!("settings.cycle_view") => cycle_view,
        t!("settings.toggle_viewport") => toggle_viewport,
    );
    section_header(ui, st, &t!("settings.section_export"));
    changed |= keybind_rows!(ui, st, hint, km, defaults,
        t!("settings.export_jar") => export_jar,
        t!("settings.export_decompiled") => export_decompiled,
    );
    changed
}

fn settings_action_id() -> egui::Id {
    egui::Id::new("settings_action")
}

fn queue_settings_action(ctx: &egui::Context, action: SettingsAction) {
    ctx.data_mut(|d| d.insert_temp(settings_action_id(), Some(action)));
}

fn take_settings_action(ctx: &egui::Context) -> Option<SettingsAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<SettingsAction>>(settings_action_id())
            .flatten()
    })
}

fn paint_cache_root_hint(ui: &mut egui::Ui, st: &SettingsTheme) {
    let text = match decompiler::current_cache_root() {
        Ok(path) => t!("settings.cache_current_root", path = path.display()).to_string(),
        Err(error) => t!("settings.cache_list_failed", error = error.to_string()).to_string(),
    };
    paint_path_hint_line(ui, st, text);
}

fn paint_cache_actions(ui: &mut egui::Ui, st: &SettingsTheme, state: &SettingsPanelState) {
    let fbt = theme::flat_button_theme(theme::TEXT_SECONDARY);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(
                t!("settings.cache_summary", count = state.cache_entries.len()).to_string(),
            )
            .size(11.0)
            .color(st.text_secondary),
        );
        ui.add_space(8.0);
        let enabled = !state.cache_entries.is_empty() && !state.cache_busy;
        let btn = ui.add_enabled(
            enabled,
            FlatButton::new(&t!("settings.cache_delete_all"), &fbt)
                .font_size(11.5)
                .min_size(egui::vec2(0.0, 22.0)),
        );
        if btn.clicked() {
            queue_settings_action(
                ui.ctx(),
                SettingsAction::DeleteAllCaches {
                    count: state.cache_entries.len(),
                },
            );
        }
    });
}

fn paint_cache_message(ui: &mut egui::Ui, st: &SettingsTheme, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(text)
                .size(12.0)
                .color(st.text_secondary),
        );
    });
}

fn paint_cache_entry(ui: &mut egui::Ui, st: &SettingsTheme, entry: &CacheEntry, cache_busy: bool) {
    let fbt = theme::flat_button_theme(theme::TEXT_SECONDARY);
    let avail_w = ui.available_width();
    let row_height = 44.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(avail_w, row_height), egui::Sense::hover());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, st.bg_hover);
    }
    let mid_y = rect.center().y;
    let left = rect.left() + 16.0;
    let painter = ui.painter();
    // 状态色点
    let dot_color = if entry.complete {
        st.accent
    } else {
        st.text_muted
    };
    painter.circle_filled(egui::pos2(left + 3.0, mid_y), 3.0, dot_color);
    // JAR 名称
    let name_x = left + 16.0;
    painter.text(
        egui::pos2(name_x, mid_y - 7.0),
        egui::Align2::LEFT_CENTER,
        &entry.jar_name,
        egui::FontId::proportional(12.5),
        st.text_primary,
    );
    // 次级信息：大小 + hash
    let mode_text = match entry.kotlin_mode {
        decompiler::KotlinDecompilerMode::Vineflower => t!("settings.kotlin_decompiler_vineflower_short").to_string(),
        decompiler::KotlinDecompilerMode::Java => t!("settings.kotlin_decompiler_java_short").to_string(),
    };
    let meta_text = format!(
        "{}  {}  {}",
        &mode_text,
        format_optional_bytes(entry.size_bytes),
        short_hash(&entry.hash),
    );
    painter.text(
        egui::pos2(name_x, mid_y + 9.0),
        egui::Align2::LEFT_CENTER,
        &meta_text,
        egui::FontId::monospace(10.0),
        st.text_muted,
    );
    // 删除按钮（右侧）
    let btn_w = 22.0;
    let btn_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 16.0 - btn_w * 0.5, mid_y),
        egui::vec2(btn_w, btn_w),
    );
    let mut btn_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(btn_rect)
            .id_salt(egui::Id::new("cache_del").with(entry.dir.display().to_string())),
    );
    let delete = btn_ui.add_enabled(
        !cache_busy,
        FlatButton::new(codicon::CLOSE, &fbt)
            .font_size(12.0)
            .font_family(codicon::family())
            .min_size(egui::vec2(btn_w, btn_w)),
    );
    if delete.clicked() {
        queue_settings_action(
            ui.ctx(),
            SettingsAction::DeleteCache {
                dir: entry.dir.clone(),
                label: format!("{} ({})", entry.jar_name.as_str(), mode_text),
            },
        );
    }
}

fn short_hash(hash: &str) -> &str {
    &hash[..16.min(hash.len())]
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_optional_bytes(bytes: Option<u64>) -> String {
    bytes
        .map(format_bytes)
        .unwrap_or_else(|| t!("settings.cache_size_unknown").to_string())
}
