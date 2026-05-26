//! Codicon 字体：caption button 图标 + 字体族注册
//!
//! @author sky

use eframe::egui;

/// 关闭按钮 U+EAB8
#[cfg(not(target_os = "macos"))]
pub const CHROME_CLOSE: &str = "\u{EAB8}";
/// 最大化按钮 U+EAB9
#[cfg(not(target_os = "macos"))]
pub const CHROME_MAXIMIZE: &str = "\u{EAB9}";
/// 最小化按钮 U+EABA
#[cfg(not(target_os = "macos"))]
pub const CHROME_MINIMIZE: &str = "\u{EABA}";
/// 还原按钮 U+EABB
#[cfg(not(target_os = "macos"))]
pub const CHROME_RESTORE: &str = "\u{EABB}";
/// 向右箭头（子菜单展开）U+EAB6
pub const CHEVRON_RIGHT: &str = "\u{EAB6}";

/// Codicon 字体族名称
pub fn family() -> egui::FontFamily {
    egui::FontFamily::Name("codicon".into())
}
