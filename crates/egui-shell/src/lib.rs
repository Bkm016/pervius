//! egui-shell: 自定义标题栏窗口壳子 + 可复用 UI 组件
//!
//! 封装 `decorations=false` 无框窗口 + 自绘标题栏 + 跨平台 resize +
//! Windows DWM 圆角。业务层只需实现 `AppContent` trait。
//!
//! 包含浮动窗口（`components::window`）和设置面板（`components::settings`）组件。
//!
//! @author sky

mod app;
mod codicon;
pub mod components;
mod fonts;
mod platform;
mod titlebar;

pub use app::{AppContent, ShellOptions, ShellTheme, run};
pub use codicon::family as codicon_family;
pub use components::WindowConfig;
