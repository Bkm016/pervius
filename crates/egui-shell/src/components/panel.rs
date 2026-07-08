//! 面板组件：菜单、设置面板、状态栏
//!
//! @author sky

pub mod menu;
pub mod settings;
pub mod status_bar;

pub use menu::{MenuTheme, menu_item, menu_item_if, menu_item_raw, menu_item_raw_if, menu_submenu};
pub use settings::{
    SectionDef, SettingsFile, SettingsPanel, SettingsTheme, dropdown, is_recording_keybind,
    keybind_row, keybind_row_with, path_picker, path_picker_with, section_header, sidebar_item,
    slider, toggle,
};
pub use status_bar::{
    Alignment, ProgressItem, StatusBarTheme, StatusBarWidget, StatusItem, TextItem,
};
