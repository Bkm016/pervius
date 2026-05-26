//! 状态栏模块
//!
//! @author sky

mod bar;
mod class_info;
mod decompile_progress;
mod download_progress;
mod export_progress;
mod index_progress;
mod modified_count;
mod view_toggle;

use crate::app::workspace::DecompilePhase;
use crate::app::App;
pub use bar::StatusBar;
use eframe::egui;
use std::sync::atomic::Ordering;

impl App {
    /// 将 App 业务状态同步到 StatusBar 显示状态
    pub(crate) fn sync_status_bar(&mut self, ctx: &egui::Context) {
        let class_info = self
            .layout
            .editor
            .focused_class_info()
            .map(|s| s.to_string());
        self.layout.status_bar.sync_view(
            self.layout.editor.focused_view(),
            self.layout.editor.focused_is_class(),
            class_info.as_deref(),
        );
        let saved_paths: Vec<String> = self
            .workspace
            .jar()
            .map(|j| j.modified_paths().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let unsaved_paths = self.layout.editor.unsaved_paths();
        self.layout
            .status_bar
            .sync_modified_count(saved_paths, unsaved_paths);
        // 外部工具下载进度（英文固定文案）
        let download_info = pervius_java_bridge::environment::download_progress();
        self.layout
            .status_bar
            .sync_download(download_info.clone());
        // 反编译进度
        let re_decompile_name = self
            .workspace
            .loaded()
            .and_then(|s| s.pending_re_decompile.as_ref())
            .map(|(name, _)| name.as_str());
        if !self.pending_compiles.is_empty() {
            let name = &self.pending_compiles.last().unwrap().entry_path;
            let short = name.rsplit('/').next().unwrap_or(name);
            self.layout.status_bar.sync_compile_single(short);
        } else if let Some(name) = re_decompile_name {
            self.layout.status_bar.sync_decompile_single(name);
        } else if !self.pending_decompiles.is_empty() {
            let name = &self.pending_decompiles.last().unwrap().0;
            let short = name.rsplit('/').next().unwrap_or(name);
            self.layout.status_bar.sync_decompile_single(short);
        } else {
            let decompile_info = if let Some(loaded) = self.workspace.loaded() {
                if let DecompilePhase::Running { task, .. } = &loaded.decompile {
                    let current = task.progress.current.load(Ordering::Relaxed);
                    let total = task.progress.total.load(Ordering::Relaxed);
                    Some((task.jar_name.clone(), current, total))
                } else {
                    None
                }
            } else {
                None
            };
            self.layout.status_bar.sync_decompile(
                decompile_info
                    .as_ref()
                    .map(|(n, c, t)| (n.as_str(), *c, *t)),
            );
        }
        // 导出进度
        let export_info = self.exporting.as_ref().map(|state| {
            let current = state.progress.current.load(Ordering::Relaxed);
            let total = state.progress.total.load(Ordering::Relaxed);
            (current, total)
        });
        self.layout.status_bar.sync_export(export_info);
        // 搜索索引构建进度
        let index_info = self.workspace.loaded().and_then(|s| {
            let p = s.search_index_progress.as_ref()?;
            let current = p.load(Ordering::Relaxed);
            Some((current, s.search_index_total))
        });
        self.layout.status_bar.sync_index(index_info);
        // 有后台任务运行时持续刷新
        let has_bg_work = download_info.is_some()
            || self.pending_vineflower_prepare.is_some()
            || self.workspace.is_decompiling()
            || !self.pending_decompiles.is_empty()
            || !self.pending_compiles.is_empty()
            || self
                .workspace
                .loaded()
                .is_some_and(|s| s.pending_re_decompile.is_some())
            || self.exporting.is_some()
            || self
                .workspace
                .loaded()
                .is_some_and(|s| s.search_index_task.is_some());
        if has_bg_work {
            ctx.request_repaint();
        }
    }
}
