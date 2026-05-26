//! 左侧面板：文件树
//!
//! @author sky

mod filter;
pub mod tree;

use crate::appearance::theme::flat_button_theme;
use crate::appearance::{codicon, theme};
use crate::task::Task;
use eframe::egui;
use egui_shell::components::{menu_item_raw, FlatButton};
use rust_i18n::t;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tree::TreeNode;

/// Classpath 面板产生的一次性动作（由 App 消费）。
#[derive(Clone)]
pub enum ClasspathAction {
    /// 为当前项目/会话添加 classpath。
    AddProject,
    /// 在文件管理器中显示 classpath 条目。
    RevealPath(PathBuf),
    /// 删除当前项目/会话 classpath。
    RemoveProject(PathBuf),
    /// 删除全局 classpath 配置。
    RemoveGlobal(String),
}

tabookit::class! {
    /// 文件面板状态
    pub struct FilePanel {
        /// 根目录（文件树顶部）
        pub roots: Vec<TreeNode>,
        /// 当前选中项（路径）
        pub selected: Option<String>,
        /// 待打开的文件条目路径（由 App 消费）
        pub pending_open: Option<String>,
        /// 待定位的文件条目路径（由 App 消费，在资源管理器中打开）
        pub pending_reveal: Option<String>,
        /// 需要滚动到选中项
        pub scroll_to_selected: bool,
        /// 速搜过滤文本（键盘直接输入，IntelliJ 风格）
        pub filter: String,
        /// 面板是否拥有隐式键盘焦点（点击面板获得，点击别处失去）
        focused: bool,
        /// 预构建的过滤索引（树变化时重建，Arc 共享给后台线程）
        filter_index: Option<Arc<Vec<tree::FilterEntry>>>,
        /// 当前过滤可见集合（后台线程产出）
        filter_visible: HashSet<String>,
        /// 后台过滤任务
        filter_task: Option<Task<(u64, tree::FilterResult)>>,
        /// 过滤请求计数器（丢弃过期结果）
        filter_gen: u64,
        /// Classpath 面板是否展开
        classpath_expanded: bool,
        /// Classpath 面板当前高度（0 表示使用自动高度）
        classpath_height: f32,
        /// 待处理 Classpath UI 动作
        pending_classpath_action: Option<ClasspathAction>,
    }

    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            selected: None,
            pending_open: None,
            pending_reveal: None,
            scroll_to_selected: false,
            filter: String::new(),
            focused: false,
            filter_index: None,
            filter_visible: HashSet::new(),
            filter_task: None,
            filter_gen: 0,
            classpath_expanded: true,
            classpath_height: 0.0,
            pending_classpath_action: None,
        }
    }

    /// 在给定 rect 内渲染（背景由 layout island 绘制）
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        tab_modified: &HashSet<String>,
        jar_modified: &HashSet<String>,
        decompiled_classes: Option<&HashSet<String>>,
        current_jar: Option<&Path>,
        project_classpath: &[PathBuf],
        global_classpath: &[String],
    ) {
        let rect = ui.max_rect();
        self.update_focus(ui.ctx(), rect);
        if self.focused {
            self.capture_input(ui.ctx());
        }
        self.poll_filter_result();
        // 面板标题
        let title_h = 32.0;
        let title_rect =
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), title_h));
        ui.painter().text(
            egui::pos2(title_rect.left() + 12.0, title_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &t!("explorer.title"),
            egui::FontId::proportional(11.0),
            theme::TEXT_SECONDARY,
        );
        // 标题栏右侧按钮
        self.render_title_buttons(ui, title_rect);
        // 内容区（左 2px、右 8px padding，右侧留空避免文字贴进 scrollbar）
        let body_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 2.0, title_rect.bottom()),
            egui::pos2(rect.right() - 8.0, rect.bottom()),
        );
        let classpath_rect =
            self.classpath_rect(body_rect, current_jar, project_classpath, global_classpath);
        let tree_rect = egui::Rect::from_min_max(
            body_rect.left_top(),
            egui::pos2(body_rect.right(), classpath_rect.top()),
        );
        self.render_classpath_resize_handle(ui, body_rect, classpath_rect);
        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tree_rect)
                .id(egui::Id::new("explorer_body")),
        );
        self.render_tree(&mut body_ui, tab_modified, jar_modified, decompiled_classes);
        self.render_classpath_panel(ui, classpath_rect, current_jar, project_classpath, global_classpath);
        // 过滤条浮层
        self.render_filter_bar(ui, rect);
    }

    /// 取出 Classpath 面板动作。
    pub fn take_classpath_action(&mut self) -> Option<ClasspathAction> {
        self.pending_classpath_action.take()
    }

    fn classpath_height_bounds(body_rect: egui::Rect) -> (f32, f32) {
        let max_height = (body_rect.height() - 48.0).max(32.0);
        let min_height = 58.0_f32.min(max_height);
        (min_height, max_height)
    }

    fn classpath_rect(
        &self,
        body_rect: egui::Rect,
        current_jar: Option<&Path>,
        project_classpath: &[PathBuf],
        global_classpath: &[String],
    ) -> egui::Rect {
        let entries = current_jar.map_or(0, |_| 1) + project_classpath.len() + global_classpath.len();
        let auto_height = 34.0 + entries.max(1) as f32 * 24.0 + 8.0;
        let (min_height, max_height) = Self::classpath_height_bounds(body_rect);
        let height = if self.classpath_expanded {
            let preferred = if self.classpath_height > 0.0 {
                self.classpath_height
            } else {
                auto_height
            };
            preferred.clamp(min_height, max_height)
        } else {
            32.0_f32.min(max_height)
        };
        egui::Rect::from_min_max(
            egui::pos2(body_rect.left(), body_rect.bottom() - height),
            body_rect.right_bottom(),
        )
    }

    fn render_classpath_resize_handle(
        &mut self,
        ui: &mut egui::Ui,
        body_rect: egui::Rect,
        classpath_rect: egui::Rect,
    ) {
        if !self.classpath_expanded {
            return;
        }
        let handle_rect = egui::Rect::from_min_max(
            egui::pos2(body_rect.left() + 6.0, classpath_rect.top() - 3.0),
            egui::pos2(body_rect.right(), classpath_rect.top() + 3.0),
        );
        let resp = ui.interact(
            handle_rect,
            egui::Id::new("classpath_resize_handle"),
            egui::Sense::drag(),
        );
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            ui.painter().line_segment(
                [
                    egui::pos2(handle_rect.left(), classpath_rect.top()),
                    egui::pos2(handle_rect.right(), classpath_rect.top()),
                ],
                egui::Stroke::new(2.0, theme::VERDIGRIS.linear_multiply(0.9)),
            );
        }
        if resp.dragged() {
            let (min_height, max_height) = Self::classpath_height_bounds(body_rect);
            let base_height = if self.classpath_height > 0.0 {
                self.classpath_height
            } else {
                classpath_rect.height()
            };
            self.classpath_height = (base_height - resp.drag_delta().y).clamp(min_height, max_height);
        }
    }

    fn render_classpath_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        current_jar: Option<&Path>,
        project_classpath: &[PathBuf],
        global_classpath: &[String],
    ) {
        ui.painter().line_segment(
            [egui::pos2(rect.left() + 6.0, rect.top()), egui::pos2(rect.right(), rect.top())],
            egui::Stroke::new(1.0, theme::BORDER),
        );
        let header_h = 32.0;
        let header_rect = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), header_h));
        let header_resp = ui.interact(header_rect, egui::Id::new("classpath_header"), egui::Sense::click());
        if header_resp.clicked() {
            self.classpath_expanded = !self.classpath_expanded;
        }
        let arrow = if self.classpath_expanded { codicon::CHEVRON_DOWN } else { codicon::CHEVRON_RIGHT };
        ui.painter().text(
            egui::pos2(header_rect.left() + 10.0, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            arrow,
            egui::FontId::new(12.0, codicon::family()),
            theme::TEXT_MUTED,
        );
        ui.painter().text(
            egui::pos2(header_rect.left() + 28.0, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            t!("explorer.classpath_title").to_string(),
            egui::FontId::proportional(11.0),
            theme::TEXT_SECONDARY,
        );
        let fbt = flat_button_theme(theme::TEXT_SECONDARY);
        let btn_size = egui::vec2(22.0, 22.0);
        let add_rect = egui::Rect::from_center_size(
            egui::pos2(header_rect.right() - 12.0 - btn_size.x * 0.5, header_rect.center().y),
            btn_size,
        );
        let mut add_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(add_rect)
                .id_salt("classpath_add_btn"),
        );
        if add_ui
            .add(
                FlatButton::new(codicon::ADD, &fbt)
                    .font_size(14.0)
                    .font_family(codicon::family())
                    .inactive_color(theme::TEXT_SECONDARY)
                    .min_size(btn_size),
            )
            .on_hover_text(t!("explorer.classpath_add"))
            .clicked()
        {
            self.pending_classpath_action = Some(ClasspathAction::AddProject);
        }
        if !self.classpath_expanded {
            return;
        }
        let list_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), header_rect.bottom()),
            rect.right_bottom(),
        );
        let mut list_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(list_rect)
                .id(egui::Id::new("classpath_list")),
        );
        egui::ScrollArea::vertical()
            .id_salt("classpath_scroll")
            .auto_shrink(false)
            .show(&mut list_ui, |ui| {
                self.render_classpath_entries(ui, current_jar, project_classpath, global_classpath);
            });
    }

    fn render_classpath_entries(
        &mut self,
        ui: &mut egui::Ui,
        current_jar: Option<&Path>,
        project_classpath: &[PathBuf],
        global_classpath: &[String],
    ) {
        let mut any = false;
        if let Some(path) = current_jar {
            any = true;
            self.render_classpath_row(
                ui,
                &path.to_string_lossy(),
                Some(t!("explorer.classpath_tag_jar").to_string()),
                theme::ACCENT_GREEN,
                None,
            );
        }
        for path in project_classpath {
            any = true;
            self.render_classpath_row(
                ui,
                &path.to_string_lossy(),
                Some(t!("explorer.classpath_tag_project").to_string()),
                theme::TEXT_MUTED,
                Some(ClasspathAction::RemoveProject(path.clone())),
            );
        }
        for entry in global_classpath {
            any = true;
            self.render_classpath_row(
                ui,
                entry,
                Some(t!("explorer.classpath_tag_global").to_string()),
                theme::ACCENT_CYAN,
                Some(ClasspathAction::RemoveGlobal(entry.clone())),
            );
        }
        if any {
            return;
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!("explorer.classpath_empty").to_string())
                .size(11.0)
                .color(theme::TEXT_MUTED),
        );
    }

    fn render_classpath_row(
        &mut self,
        ui: &mut egui::Ui,
        path: &str,
        tag: Option<String>,
        tag_color: egui::Color32,
        remove_action: Option<ClasspathAction>,
    ) {
        let row_h = 24.0;
        let avail_w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(avail_w, row_h), egui::Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 3.0, theme::BG_HOVER);
        }
        let tag_w = if tag.is_some() { 54.0 } else { 0.0 };
        let exists = Path::new(path).exists();
        let path_buf = Path::new(path);
        let file_name = path_buf
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(path);
        let path_detail = path_buf
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.display().to_string())
            .unwrap_or_default();
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 10.0, rect.top()),
            egui::pos2(rect.right() - tag_w - 8.0, rect.bottom()),
        );
        let mut text_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center))
                .id_salt(("classpath_row_text", path)),
        );
        text_ui.add(
            egui::Label::new(
                egui::RichText::new(file_name)
                    .size(11.0)
                    .strong()
                    .color(if exists { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED }),
            )
            .truncate(),
        );
        if !path_detail.is_empty() {
            text_ui.add_space(6.0);
            text_ui.add(
                egui::Label::new(
                    egui::RichText::new(path_detail)
                        .size(10.5)
                        .monospace()
                        .color(if exists { theme::TEXT_SECONDARY } else { theme::TEXT_MUTED }),
                )
                .truncate(),
            );
        }
        if let Some(tag) = tag {
            let tag_rect = egui::Rect::from_min_max(
                egui::pos2(rect.right() - tag_w, rect.top() + 4.0),
                egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0),
            );
            ui.painter().rect_filled(tag_rect, 3.0, theme::BG_MEDIUM);
            ui.painter().text(
                tag_rect.center(),
                egui::Align2::CENTER_CENTER,
                tag,
                egui::FontId::proportional(9.0),
                tag_color,
            );
        }
        if exists || remove_action.is_some() {
            resp.context_menu(|ui| {
                ui.style_mut().visuals.widgets.hovered.bg_fill = theme::BG_HOVER;
                if exists && menu_item_raw(ui, &theme::menu_theme(), &t!("explorer.reveal"), "") {
                    self.pending_classpath_action = Some(ClasspathAction::RevealPath(path_buf.to_path_buf()));
                    ui.close();
                }
                if exists && remove_action.is_some() {
                    ui.separator();
                }
                if let Some(action) = &remove_action {
                    if menu_item_raw(ui, &theme::menu_theme(), &t!("explorer.classpath_remove"), "") {
                        self.pending_classpath_action = Some(action.clone());
                        ui.close();
                    }
                }
            });
        }
    }

    fn render_tree(
        &mut self,
        ui: &mut egui::Ui,
        tab_modified: &HashSet<String>,
        jar_modified: &HashSet<String>,
        decompiled_classes: Option<&HashSet<String>>,
    ) {
        let filtering = !self.filter.is_empty();
        let mut ctx_reveal = None;
        let scroll = self.scroll_to_selected;
        let opened = tree::render_tree(
            ui,
            &mut self.roots,
            &self.selected,
            &self.filter_visible,
            &mut ctx_reveal,
            scroll,
            tab_modified,
            jar_modified,
            decompiled_classes,
        );
        self.scroll_to_selected = false;
        if let Some(path) = opened {
            self.selected = Some(path.clone());
            self.pending_open = Some(path.clone());
            if filtering {
                tree::reveal(&mut self.roots, &path);
                self.scroll_to_selected = true;
            }
            self.clear_filter();
        }
        if ctx_reveal.is_some() {
            self.pending_reveal = ctx_reveal;
        }
    }

    /// 标题栏右侧展开/折叠按钮
    fn render_title_buttons(&mut self, ui: &mut egui::Ui, title_rect: egui::Rect) {
        if self.roots.is_empty() {
            return;
        }
        let fbt = flat_button_theme(theme::TEXT_SECONDARY);
        let btn_size = egui::vec2(22.0, 22.0);
        let mid_y = title_rect.center().y;
        let icon_family = codicon::family();
        // 折叠按钮（最右）
        let collapse_x = title_rect.right() - 8.0 - btn_size.x * 0.5;
        let collapse_rect = egui::Rect::from_center_size(egui::pos2(collapse_x, mid_y), btn_size);
        let mut collapse_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(collapse_rect)
                .id_salt("collapse_btn"),
        );
        if collapse_ui
            .add(
                FlatButton::new(codicon::COLLAPSE_ALL, &fbt)
                    .font_size(14.0)
                    .font_family(icon_family.clone())
                    .inactive_color(theme::TEXT_SECONDARY)
                    .min_size(btn_size),
            )
            .on_hover_text(t!("explorer.collapse"))
            .clicked()
        {
            tree::collapse_one_level(&mut self.roots);
        }
        // 展开按钮
        let expand_x = collapse_rect.left() - 2.0 - btn_size.x * 0.5;
        let expand_rect = egui::Rect::from_center_size(egui::pos2(expand_x, mid_y), btn_size);
        let mut expand_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(expand_rect)
                .id_salt("expand_btn"),
        );
        if expand_ui
            .add(
                FlatButton::new(codicon::EXPAND_ALL, &fbt)
                    .font_size(14.0)
                    .font_family(icon_family)
                    .inactive_color(theme::TEXT_SECONDARY)
                    .min_size(btn_size),
            )
            .on_hover_text(t!("explorer.expand"))
            .clicked()
        {
            tree::expand_one_level(&mut self.roots);
        }
    }
}

use crate::app::App;

impl App {
    /// 编辑器聚焦 tab 变化时同步 explorer 选中状态
    pub(crate) fn sync_explorer_selection(&mut self) {
        if let Some(path) = self.layout.editor.focused_entry_path() {
            if self.layout.file_panel.selected.as_ref() != Some(&path) {
                tree::reveal(&mut self.layout.file_panel.roots, &path);
                self.layout.file_panel.selected = Some(path);
                self.layout.file_panel.scroll_to_selected = true;
            }
        }
    }

    /// 分别收集 tab 级别（橙色）和 JAR 级别（绿色）已修改条目路径（含父级目录）
    pub(crate) fn split_modified_entries(&self) -> (HashSet<String>, HashSet<String>) {
        let mut tab_set = HashSet::new();
        let mut jar_set = HashSet::new();
        for (_, tab) in self.layout.editor.dock_state.iter_all_tabs() {
            if tab.is_modified || tab.source_modified {
                if let Some(path) = &tab.entry_path {
                    Self::insert_with_parents(&mut tab_set, path);
                }
            }
        }
        if let Some(jar) = self.workspace.jar() {
            for path in jar.modified_paths() {
                Self::insert_with_parents(&mut jar_set, path);
            }
        }
        (tab_set, jar_set)
    }

    /// 将路径及其所有父级目录加入集合
    fn insert_with_parents(set: &mut HashSet<String>, path: &str) {
        set.insert(path.to_string());
        let mut p = path;
        while let Some(idx) = p.rfind('/') {
            let parent = &p[..idx + 1];
            if !set.insert(parent.to_string()) {
                break;
            }
            p = &p[..idx];
        }
        set.insert(String::new());
    }
}
