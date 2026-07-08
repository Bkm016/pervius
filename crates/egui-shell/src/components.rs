//! 可复用 UI 组件：浮动窗口、设置面板、基础控件
//!
//! @author sky

pub mod overlay;
pub mod panel;
pub mod widget;

use eframe::egui;

pub use overlay::{ConfirmDialog, ConfirmResult, ConfirmTheme, FloatingWindow};
pub use panel::{
    Alignment, MenuTheme, ProgressItem, SectionDef, SettingsFile, SettingsPanel, SettingsTheme,
    StatusBarTheme, StatusBarWidget, StatusItem, TextItem, dropdown, is_recording_keybind,
    keybind_row, keybind_row_with, menu_item, menu_item_if, menu_item_raw, menu_item_raw_if,
    menu_submenu, path_picker, path_picker_with, section_header, sidebar_item, slider, toggle,
};
pub use widget::{FlatButton, FlatButtonTheme, IslandStyle};

/// 浮动窗口专属配置（ShellTheme.window）
#[derive(Clone)]
pub struct WindowConfig {
    /// 窗口外框样式（fill / stroke / corner_radius / shadow）
    pub frame: egui::Frame,
    /// header 区域高度
    pub header_height: f32,
    /// Pin 图标字符
    pub pin_icon: &'static str,
    /// Pin 按钮 tooltip（已固定时）
    pub pin_tooltip: String,
    /// Unpin 按钮 tooltip（未固定时）
    pub unpin_tooltip: String,
}

/// 1px 水平分隔线
pub fn separator(ui: &mut egui::Ui, color: egui::Color32) {
    let avail = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [
            egui::pos2(avail.left(), avail.top()),
            egui::pos2(avail.right(), avail.top()),
        ],
        egui::Stroke::new(1.0, color),
    );
    ui.allocate_space(egui::vec2(avail.width(), 1.0));
}
