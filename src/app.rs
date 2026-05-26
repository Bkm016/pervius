//! 应用核心：业务状态 + 业务逻辑调度
//!
//! @author sky

mod compile;
mod decompile;
mod export;
pub(crate) mod navigate;
mod open;
mod platform;
mod search;
mod tab;
pub(crate) mod workspace;

pub use crate::ui::confirm::ConfirmAction;

use crate::settings::Settings;
use crate::task::Task;
use crate::ui::layout::Layout;
use egui_editor::highlight::Language;
use egui_notify::Toasts;
use egui_shell::components::SettingsFile;
use pervius_java_bridge::decompiler::{CachedSource, DecompiledSourceLanguage};
use pervius_java_bridge::error::BridgeError;
use workspace::Workspace;

pub(crate) use export::ExportingState;

pub(crate) fn editor_source_language(language: DecompiledSourceLanguage) -> Language {
    match language {
        DecompiledSourceLanguage::Java => Language::Java,
        DecompiledSourceLanguage::Kotlin => Language::Kotlin,
    }
}

pub(crate) enum CacheDeleteResult {
    Single {
        label: String,
        deleted: bool,
    },
    All {
        count: usize,
    },
}

/// 应用核心业务状态
pub struct App {
    /// UI 布局状态
    pub layout: Layout,
    /// 应用设置
    pub settings: Settings,
    /// 通知提示
    pub toasts: Toasts,
    /// 待确认的破坏性动作
    pub pending_confirm: Option<ConfirmAction>,
    /// 工作区状态（Empty / Loading / Loaded）
    pub(crate) workspace: Workspace,
    /// 单文件反编译结果队列（支持并发，独立文件不依赖 JAR）
    pub(crate) pending_decompiles: Vec<(String, Task<Result<CachedSource, BridgeError>>)>,
    /// class 源码编译结果队列
    pub(crate) pending_compiles: Vec<compile::PendingCompile>,
    /// 后台缓存删除任务
    pub(crate) pending_cache_delete: Option<Task<CacheDeleteResult>>,
    /// 后台 Vineflower 预热任务（设置修改后立即下载/校验）
    pub(crate) pending_vineflower_prepare: Option<Task<Result<std::path::PathBuf, BridgeError>>>,
    /// 后台 JAR 导出任务（快照已取，可跨 JAR 切换存活）
    pub(crate) exporting: Option<ExportingState>,
}

impl App {
    pub fn new() -> Self {
        let settings = Settings::load();
        apply_bridge_settings(&settings);
        Self {
            layout: Layout::new(&settings),
            settings,
            toasts: Toasts::default(),
            pending_confirm: None,
            workspace: Workspace::Empty,
            pending_decompiles: Vec::new(),
            pending_compiles: Vec::new(),
            pending_cache_delete: None,
            pending_vineflower_prepare: None,
            exporting: None,
        }
    }

    /// 打开设置对话框
    pub fn open_settings(&mut self) {
        crate::settings::refresh_cache_state(&mut self.layout.settings_state);
        self.layout.settings_panel.open(&self.settings);
    }
}

fn apply_bridge_settings(settings: &Settings) {
    // 传递用户配置给 bridge 层
    pervius_java_bridge::process::set_java_home(&settings.java.java_home);
    pervius_java_bridge::decompiler::set_cache_root(settings.cache.root_path());
    pervius_java_bridge::decompiler::set_kotlin_decompiler_mode(
        settings.compile.kotlin_decompiler.to_bridge(),
    );
    pervius_java_bridge::environment::set_environment_config(settings.java.environment_config());
}
