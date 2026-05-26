//! 状态栏服务：自管理 items 注册、渲染、与编辑器的状态同步
//!
//! @author sky

use super::class_info::ClassInfoItem;
use super::decompile_progress::DecompileProgressItem;
use super::download_progress::DownloadProgressItem;
use super::export_progress::ExportProgressItem;
use super::index_progress::IndexProgressItem;
use super::modified_count::ModifiedCountItem;
use super::view_toggle::ViewToggleItem;
use crate::appearance::theme;
use crate::ui::editor::view_toggle::ActiveView;
use eframe::egui;
use egui_shell::components::panel::status_bar::{Alignment, StatusBarWidget, StatusItem, TextItem};
use pervius_java_bridge::environment::DownloadProgressSnapshot;
use rust_i18n::t;

tabookit::class! {
    /// 状态栏服务
    ///
    /// 内置默认 items（应用版本、类信息、视图切换与后台进度），
    /// 外部只需调用 `render` 和 `sync` 即可。
    pub struct StatusBar {
        widget: StatusBarWidget,
    }

    /// 获取指定类型的 item 可变引用
    fn item_mut<T: StatusItem>(&mut self) -> Option<&mut T> {
        self.widget.item_mut::<T>()
    }

    /// 同步编辑器状态（ViewToggle + ClassInfo + context_only items 可见性）
    pub fn sync_view(
        &mut self,
        active_view: Option<ActiveView>,
        is_class: bool,
        class_info: Option<&str>,
    ) {
        let has_tab = active_view.is_some();
        for item in self.widget.items_mut() {
            if let Some(text) = item.downcast_mut::<TextItem>() {
                if text.is_context_only() {
                    text.set_visible(has_tab);
                }
            }
            // 三视图切换仅对 .class 文件显示
            if let Some(vt) = item.downcast_mut::<ViewToggleItem>() {
                vt.set_active(if is_class { active_view } else { None });
            }
            // class 版本信息
            if let Some(ci) = item.downcast_mut::<ClassInfoItem>() {
                ci.set_info(class_info);
            }
        }
    }

    /// 取出用户通过 ViewToggle 切换的新视图（渲染后调用）
    pub fn take_view_change(&mut self) -> Option<ActiveView> {
        self.item_mut::<ViewToggleItem>()?.take_changed()
    }

    /// 同步单文件反编译状态
    pub fn sync_decompile_single(&mut self, name: &str) {
        if let Some(item) = self.item_mut::<DecompileProgressItem>() {
            item.set_single(name);
        }
    }

    /// 同步源码编译状态
    pub fn sync_compile_single(&mut self, name: &str) {
        if let Some(item) = self.item_mut::<DecompileProgressItem>() {
            item.set_compile(name);
        }
    }

    /// 同步批量反编译进度，None 表示无任务
    pub fn sync_decompile(&mut self, info: Option<(&str, u32, u32)>) {
        if let Some(item) = self.item_mut::<DecompileProgressItem>() {
            item.set_progress(info);
        }
    }

    /// 同步外部工具下载进度，None 表示无任务
    pub fn sync_download(&mut self, progress: Option<DownloadProgressSnapshot>) {
        if let Some(item) = self.item_mut::<DownloadProgressItem>() {
            item.set_progress(progress);
        }
    }

    /// 同步已修改文件路径
    pub fn sync_modified_count(&mut self, saved: Vec<String>, unsaved: Vec<String>) {
        if let Some(item) = self.item_mut::<ModifiedCountItem>() {
            item.set_paths(saved, unsaved);
        }
    }

    /// 同步 JAR 导出进度，None 表示无任务
    pub fn sync_export(&mut self, info: Option<(u32, u32)>) {
        if let Some(item) = self.item_mut::<ExportProgressItem>() {
            item.set_progress(info);
        }
    }

    /// 同步搜索索引构建进度，None 表示无任务
    pub fn sync_index(&mut self, info: Option<(u32, u32)>) {
        if let Some(item) = self.item_mut::<IndexProgressItem>() {
            item.set_progress(info);
        }
    }

    /// 取出弹出列表中用户点击的文件路径
    pub fn take_clicked_file(&mut self) -> Option<String> {
        self.item_mut::<ModifiedCountItem>()?.take_clicked()
    }

    /// 渲染状态栏
    pub fn render(&mut self, ui: &mut egui::Ui) {
        self.widget.render(ui);
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        let mut widget = StatusBarWidget::new(theme::status_bar_theme());
        widget.add(TextItem::new(
            t!("status.version"),
            theme::TEXT_MUTED,
            Alignment::Left,
        ));
        widget.add(ClassInfoItem::new());
        widget.add(ModifiedCountItem::new());
        widget.add(ViewToggleItem::new());
        widget.add(DownloadProgressItem::new());
        widget.add(DecompileProgressItem::new());
        widget.add(ExportProgressItem::new());
        widget.add(IndexProgressItem::new());
        Self { widget }
    }
}
