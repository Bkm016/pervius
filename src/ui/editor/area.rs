//! 编辑器区域：egui_dock 容器 + tab 管理
//!
//! @author sky

use super::render::{self, line_number_width};
use super::style::dock;
use super::tab::EditorTab;
use super::view_toggle::ActiveView;
use super::viewer::{EditorTabViewer, PendingNavigate, TabAction};
use crate::appearance::theme;
use eframe::egui;
use egui_dock::{DockArea, DockState, SurfaceIndex};
use egui_editor::find_bar::FindBar;
use pervius_java_bridge::class_structure::SavedMember;
use pervius_java_bridge::decompiler;
use rust_i18n::t;
use std::collections::{HashMap, HashSet};

tabookit::class! {
    /// 编辑器区域状态
    pub struct EditorArea {
        /// egui-dock 管理的 tab 布局
        pub dock_state: DockState<EditorTab>,
        /// 编辑器内查找栏
        pub find_bar: FindBar,
        /// 被 is_modified 拦截的关闭动作，由 App 消费并弹出确认对话框
        pub blocked_close: Option<TabAction>,
        /// 已保存成员记录（entry_path → 成员集合），跨 tab 关闭/重开保留
        pub saved_members: HashMap<String, HashSet<SavedMember>>,
        /// Shift+Click 导航请求（由 viewer 产生，由 App 消费）
        pub pending_navigate: Option<PendingNavigate>,
        /// 源码编辑右键菜单触发的立即编译请求
        pub pending_recompile: Option<String>,
    }
    pub fn new() -> Self {
        Self {
            dock_state: DockState::new(vec![]),
            find_bar: FindBar::new(),
            blocked_close: None,
            saved_members: HashMap::new(),
            pending_navigate: None,
            pending_recompile: None,
        }
    }

    /// 在给定 UI 区域内渲染
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        jar_modified: &HashSet<String>,
        known_classes: &HashSet<String>,
    ) {
        if self.is_empty() {
            Self::render_placeholder(ui);
            return;
        }
        let rect = ui.max_rect();
        if let Some(gutter_w) = self.active_gutter_width() {
            render::paint_editor_bg(ui, rect, gutter_w);
        }
        let style = dock::build(ui.style());
        let mut viewer = EditorTabViewer {
            action: None,
            find_bar: &mut self.find_bar,
            jar_modified,
            pending_close: None,
            pending_navigate: None,
            known_classes,
        };
        DockArea::new(&mut self.dock_state)
            .style(style)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_inside(ui, &mut viewer);
        let pending_close = viewer.pending_close.take();
        let action = viewer.action.take();
        let nav = viewer.pending_navigate.take();
        drop(viewer);
        if let Some(entry_path) = pending_close {
            self.blocked_close = Some(TabAction::Close(entry_path));
        }
        if let Some(action) = action {
            self.handle_tab_action(action);
        }
        if let Some(nav) = nav {
            self.pending_navigate = Some(nav);
        }
        ui.output_mut(|o| {
            if matches!(o.cursor_icon, egui::CursorIcon::Grab) {
                o.cursor_icon = egui::CursorIcon::Default;
            }
        });
    }

    fn is_empty(&self) -> bool {
        self.dock_state.main_surface().num_tabs() == 0
    }

    fn render_placeholder(ui: &mut egui::Ui) {
        use crate::ui::keybindings;
        let rect = ui.max_rect();
        let painter = ui.painter();
        let center = rect.center();
        let hint_open = t!("editor.open_file");
        let hint_find = t!("editor.find_in_files");
        let hint_shift = t!("editor.double_shift").to_string();
        let hint_project = t!("editor.project_view");
        let hints: &[(&str, String)] = &[
            (&hint_open, keybindings::DEFAULT_OPEN_JAR.label()),
            (&hint_find, hint_shift),
            (&hint_project, keybindings::DEFAULT_TOGGLE_EXPLORER.label()),
        ];
        let font_action = egui::FontId::proportional(13.0);
        let font_keybind = egui::FontId::proportional(11.0);
        let font_hint = egui::FontId::proportional(12.0);
        let line_height = 26.0;
        let gap = 16.0;
        let hints_h = line_height * hints.len() as f32;
        let total_h = hints_h + gap + line_height;
        let start_y = center.y - total_h / 2.0;
        for (i, (action, keybind)) in hints.iter().enumerate() {
            let mid_y = start_y + i as f32 * line_height + line_height / 2.0;
            painter.text(
                egui::pos2(center.x - 8.0, mid_y),
                egui::Align2::RIGHT_CENTER,
                *action,
                font_action.clone(),
                theme::TEXT_MUTED,
            );
            let keybind_galley = painter.layout_no_wrap(
                keybind.to_string(),
                font_keybind.clone(),
                theme::TEXT_MUTED,
            );
            let kb_pos = egui::pos2(center.x + 8.0, mid_y - keybind_galley.size().y / 2.0);
            let kb_rect = egui::Rect::from_min_size(kb_pos, keybind_galley.size())
                .expand2(egui::vec2(6.0, 2.0));
            painter.rect(
                kb_rect,
                egui::CornerRadius::same(3),
                theme::BG_MEDIUM,
                egui::Stroke::new(1.0, theme::BORDER),
                egui::StrokeKind::Outside,
            );
            painter.galley(kb_pos, keybind_galley, theme::TEXT_MUTED);
        }
        let drop_y = start_y + hints_h + gap + line_height / 2.0;
        painter.text(
            egui::pos2(center.x, drop_y),
            egui::Align2::CENTER_CENTER,
            &t!("editor.drop_hint"),
            font_hint,
            theme::TEXT_MUTED,
        );
        ui.allocate_rect(rect, egui::Sense::hover());
    }

    fn focused_tab(&mut self) -> Option<&mut EditorTab> {
        if self.dock_state.find_active_focused().is_some() {
            return self.dock_state.find_active_focused().map(|(_, t)| t);
        }
        self.dock_state
            .main_surface_mut()
            .find_active()
            .map(|(_, t)| t)
    }

    /// 供外部调用的聚焦 tab 可变引用
    pub fn focused_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.focused_tab()
    }

    fn active_gutter_width(&mut self) -> Option<f32> {
        let tab = self.focused_tab()?;
        match tab.active_view {
            ActiveView::Decompiled => {
                let max_number = if tab.decompiled_line_mapping.is_empty() {
                    tab.decompiled_data.line_count()
                } else {
                    tab.decompiled_line_mapping
                        .iter()
                        .filter_map(|n| n.map(|v| v as usize))
                        .max()
                        .unwrap_or(tab.decompiled_data.line_count())
                };
                Some(line_number_width(max_number))
            }
            ActiveView::Bytecode => None,
            ActiveView::Hex => None,
        }
    }

    pub fn open_tab(&mut self, mut tab: EditorTab) {
        // 恢复跨 tab 关闭/重开保留的已保存成员标记
        if let Some(entry_path) = &tab.entry_path {
            if let Some(saved) = self.saved_members.get(entry_path) {
                if let Some(cs) = &mut tab.class_structure {
                    cs.restore_saved_flags(saved);
                }
            }
        }
        self.dock_state.main_surface_mut().push_to_focused_leaf(tab);
    }

    /// 聚焦已打开的 tab（按 entry_path 匹配），返回是否找到
    pub fn focus_tab(&mut self, entry_path: &str) -> bool {
        self.focus_tab_at(entry_path, None)
    }

    /// 切换到指定 tab 并滚动到目标行
    pub fn focus_tab_at(&mut self, entry_path: &str, line: Option<usize>) -> bool {
        let found = self
            .dock_state
            .find_tab_from(|tab| tab.entry_path.as_deref() == Some(entry_path));
        let Some(tab_path) = found else {
            return false;
        };
        let _ = self.dock_state.set_active_tab(tab_path);
        if let Some(line) = line {
            if let Some(tab) = self.focused_tab() {
                tab.pending_scroll_to_line = Some(line);
            }
        }
        true
    }

    pub fn focused_view(&mut self) -> Option<ActiveView> {
        Some(self.focused_tab()?.active_view)
    }

    /// 当前聚焦 tab 是否为 .class 文件
    pub fn focused_is_class(&mut self) -> bool {
        self.focused_tab().is_some_and(|t| t.is_class)
    }

    /// 当前聚焦 tab 的 class 版本信息
    pub fn focused_class_info(&mut self) -> Option<&str> {
        self.focused_tab()?.class_info.as_deref()
    }

    /// 当前聚焦 tab 的条目路径
    pub fn focused_entry_path(&mut self) -> Option<String> {
        self.focused_tab()?.entry_path.clone()
    }

    /// 有编辑但未保存的 tab 条目路径
    pub fn unsaved_paths(&self) -> Vec<String> {
        self.dock_state
            .iter_all_tabs()
            .filter(|(_, t)| t.is_modified || t.source_modified)
            .filter_map(|(_, t)| t.entry_path.clone())
            .collect()
    }

    pub fn set_focused_view(&mut self, view: ActiveView) {
        if let Some(tab) = self.focused_tab() {
            tab.active_view = view;
        }
    }

    /// 循环切换聚焦 tab 的视图（仅 .class 文件）
    pub fn cycle_view(&mut self) {
        if let Some(tab) = self.focused_tab() {
            if !tab.is_class {
                return;
            }
            tab.active_view = tab.active_view.next();
        }
    }

    /// 打开/聚焦编辑器内查找栏
    pub fn open_find(&mut self) {
        self.find_bar.toggle();
    }

    /// 切换当前 tab 的视窗模式
    pub fn toggle_viewport(&mut self) {
        let tab = tabookit::or!(self.focused_tab(), return);
        if !(tab.is_text || (tab.is_class && tab.is_source_unlocked())) {
            return;
        }
        let currently_on = match tab.viewport_override {
            Some(v) => v,
            None => tab
                .editable_layout_cache
                .as_ref()
                .map_or(
                    tab.decompiled.len() > egui_editor::code_view::VIEWPORT_TEXT_LEN,
                    |c| c.is_viewport(),
                ),
        };
        tab.viewport_override = Some(!currently_on);
    }

    /// 反编译完成后，刷新所有已打开的 .class tab 的源码
    pub fn refresh_class_tabs(&mut self, jar_hash: Option<&str>, skip_entries: &HashSet<String>) {
        for (_, tab) in self.dock_state.iter_all_tabs_mut() {
            let entry = match &tab.entry_path {
                Some(p) if p.ends_with(".class") => p.clone(),
                _ => continue,
            };
            if skip_entries.contains(&entry) {
                continue;
            }
            let Some(hash) = jar_hash else {
                tab.set_decompiled(
                    t!("editor.decompiler_placeholder").to_string(),
                    egui_editor::Language::Java,
                    Vec::new(),
                );
                if tab.active_view == ActiveView::Decompiled {
                    tab.active_view = ActiveView::Hex;
                }
                continue;
            };
            if let Some(cached) = decompiler::cached_source(hash, &entry) {
                let lang = match cached.language {
                    decompiler::DecompiledSourceLanguage::Kotlin => egui_editor::Language::Kotlin,
                    decompiler::DecompiledSourceLanguage::Java => egui_editor::Language::Java,
                };
                tab.set_decompiled(cached.source, lang, cached.line_mapping);
                if tab.active_view == ActiveView::Hex {
                    tab.active_view = ActiveView::Decompiled;
                }
            } else {
                tab.set_decompiled(
                    t!("editor.decompiler_placeholder").to_string(),
                    egui_editor::Language::Java,
                    Vec::new(),
                );
                if tab.active_view == ActiveView::Decompiled {
                    tab.active_view = ActiveView::Hex;
                }
            }
        }
    }

    /// 关闭当前活跃 tab
    pub fn close_active_tab(&mut self) {
        let path = self.dock_state.focused_leaf().or_else(|| {
            let tree = &self.dock_state[SurfaceIndex::main()];
            if tree.is_empty() {
                return None;
            }
            let node = tree.focused_leaf()?;
            Some(egui_dock::NodePath {
                surface: SurfaceIndex::main(),
                node,
            })
        });
        if let Some(node_path) = path {
            let tree = &self.dock_state[node_path.surface];
            if tree.is_empty() || node_path.node.0 >= tree.len() {
                return;
            }
            let leaf = tree[node_path.node].get_leaf();
            let active = leaf.map(|l| l.active);
            if let Some(tab_idx) = active {
                let is_modified = leaf
                    .and_then(|l| l.tabs.get(tab_idx.0))
                    .is_some_and(|t| t.is_modified || t.source_modified);
                if is_modified {
                    let entry_path = leaf
                        .and_then(|l| l.tabs.get(tab_idx.0))
                        .and_then(|t| t.entry_path.clone());
                    self.blocked_close = Some(TabAction::Close(entry_path));
                    return;
                }
                let tab_path = egui_dock::TabPath::new(node_path.surface, node_path.node, tab_idx);
                self.dock_state.remove_tab(tab_path);
            }
        }
    }

    /// 关闭所有 tab
    pub fn close_all_tabs(&mut self) {
        if self
            .dock_state
            .iter_all_tabs()
            .any(|(_, t)| t.is_modified || t.source_modified)
        {
            self.blocked_close = Some(TabAction::CloseAll);
            return;
        }
        self.dock_state = DockState::new(vec![]);
    }

    fn handle_tab_action(&mut self, action: TabAction) {
        if self.has_modified_in_action(&action) {
            self.blocked_close = Some(action);
            return;
        }
        self.force_tab_action(action);
    }

    /// 待关闭的 tab 中是否有未保存修改
    fn has_modified_in_action(&self, action: &TabAction) -> bool {
        match action {
            TabAction::Close(path) => self
                .dock_state
                .iter_all_tabs()
                .any(|(_, t)| t.entry_path == *path && (t.is_modified || t.source_modified)),
            TabAction::CloseAll => self
                .dock_state
                .iter_all_tabs()
                .any(|(_, t)| t.is_modified || t.source_modified),
            TabAction::CloseOthers(keep) => self
                .dock_state
                .iter_all_tabs()
                .any(|(_, t)| t.entry_path != *keep && (t.is_modified || t.source_modified)),
            TabAction::RecompileSource(_) => false,
            TabAction::CloseToRight(after) => {
                let found = self.dock_state.find_tab_from(|t| t.entry_path == *after);
                match found {
                    Some(tab_path) => self.dock_state[tab_path.surface][tab_path.node]
                        .get_leaf()
                        .is_some_and(|leaf| {
                            leaf.tabs[tab_path.tab.0 + 1..]
                                .iter()
                                .any(|t| t.is_modified || t.source_modified)
                        }),
                    None => false,
                }
            }
        }
    }

    /// 强制执行 tab 关闭动作（跳过 is_modified 检查，由确认对话框调用）
    pub fn force_tab_action(&mut self, action: TabAction) {
        match action {
            TabAction::RecompileSource(path) => {
                if let Some(path) = path {
                    self.pending_recompile = Some(path);
                }
            }
            TabAction::Close(path) => {
                let found = self.dock_state.find_tab_from(|t| t.entry_path == path);
                if let Some(tab_path) = found {
                    self.dock_state.remove_tab(tab_path);
                }
            }
            TabAction::CloseAll => {
                self.dock_state = DockState::new(vec![]);
            }
            TabAction::CloseOthers(keep_path) => {
                self.dock_state
                    .retain_tabs(|tab| tab.entry_path == keep_path);
            }
            TabAction::CloseToRight(after_path) => {
                let found = self
                    .dock_state
                    .find_tab_from(|t| t.entry_path == after_path);
                if let Some(tab_path) = found {
                    let node = &mut self.dock_state[tab_path.surface][tab_path.node];
                    if let Some(leaf) = node.get_leaf_mut() {
                        leaf.tabs.truncate(tab_path.tab.0 + 1);
                        if leaf.active.0 >= leaf.tabs.len() {
                            leaf.active.0 = leaf.tabs.len().saturating_sub(1);
                        }
                    }
                }
            }
        }
    }
}
