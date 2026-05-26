//! TabViewer 实现：定义每个 Tab 的标题和内容渲染
//!
//! @author sky

use super::bytecode;
use super::render;
use super::tab::EditorTab;
use super::view_toggle::ActiveView;
use crate::appearance::{codicon, theme};
use crate::ui::keybindings;
use eframe::egui;
use egui_dock::{tab_viewer::OnCloseResponse, NodePath, TabViewer};
use egui_editor::code_view::NavigationHit;
use egui_editor::find_bar::FindBar;
use egui_shell::components::menu_item;
use rust_i18n::t;

use std::collections::HashSet;

/// Ctrl+Click 导航请求（附带来源 tab 上下文）
pub struct PendingNavigate {
    /// 导航 hit 信息
    pub hit: NavigationHit,
    /// 来源文件的 entry_path（用于解析同包类名）
    pub source_entry: Option<String>,
    /// 来源文件的反编译源码（用于解析 import 语句）
    pub source_text: String,
}

/// Tab 右键菜单触发的动作
pub enum TabAction {
    /// 关闭当前 tab
    Close(Option<String>),
    CloseAll,
    /// 关闭除指定 tab 外的所有 tab（按 entry_path 匹配）
    CloseOthers(Option<String>),
    /// 关闭指定 tab 右侧的所有 tab
    CloseToRight(Option<String>),
    /// 立即重新编译源码编辑 tab
    RecompileSource(Option<String>),
}

pub struct EditorTabViewer<'a> {
    /// 右键菜单触发的动作（每帧最多一个，由 EditorArea 消费）
    pub action: Option<TabAction>,
    /// 编辑器内查找栏状态
    pub find_bar: &'a mut FindBar,
    /// JAR 中已保存但未落盘的条目路径
    pub jar_modified: &'a HashSet<String>,
    /// on_close 被 is_modified 拦截时记录要关闭的 tab 路径
    pub pending_close: Option<Option<String>>,
    /// Ctrl+Click 导航请求（每帧最多一个，由 EditorArea 传播到 App 层）
    pub pending_navigate: Option<PendingNavigate>,
    /// JAR 内已知简短类名集合（hover 过滤用）
    pub known_classes: &'a HashSet<String>,
}

impl TabViewer for EditorTabViewer<'_> {
    type Tab = EditorTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let mut job = egui::text::LayoutJob::default();
        job.append("", 4.0, egui::TextFormat::default());
        job.append(
            codicon::JAVA,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::new(11.0, codicon::family()),
                color: theme::VERDIGRIS,
                ..Default::default()
            },
        );
        job.append(" ", 0.0, egui::TextFormat::default());
        job.append(
            &tab.title,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.0),
                color: egui::Color32::PLACEHOLDER,
                ..Default::default()
            },
        );
        if tab.is_class && tab.is_source_unlocked() {
            job.append(" ", 0.0, egui::TextFormat::default());
            job.append(
                codicon::EDIT,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::new(11.0, codicon::family()),
                    color: theme::ACCENT_CYAN,
                    ..Default::default()
                },
            );
        }
        job.append("", 4.0, egui::TextFormat::default());
        job.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        // 更新搜索（先于内容渲染，提供高亮数据）
        if self.find_bar.open {
            match tab.active_view {
                ActiveView::Hex => self.find_bar.update_bytes(&tab.raw_bytes),
                ActiveView::Decompiled => self.find_bar.update_text(&tab.decompiled),
                ActiveView::Bytecode => self.find_bar.update_text(tab.selected_bytecode_text()),
            }
        }
        let (matches, current) = if self.find_bar.open {
            self.find_bar.highlight_info()
        } else {
            (vec![], None)
        };
        // 滚动到当前匹配项
        if self.find_bar.take_scroll_request() {
            if let Some(idx) = current {
                if let Some(m) = matches.get(idx) {
                    match tab.active_view {
                        ActiveView::Hex => {
                            tab.hex_state.scroll_to_byte = Some(m.start);
                        }
                        ActiveView::Decompiled => {
                            let line = tab.decompiled[..m.start.min(tab.decompiled.len())]
                                .as_bytes()
                                .iter()
                                .filter(|&&b| b == b'\n')
                                .count();
                            tab.pending_scroll_to_line = Some(line);
                        }
                        ActiveView::Bytecode => {
                            let text = tab.selected_bytecode_text();
                            let line = text[..m.start.min(text.len())]
                                .as_bytes()
                                .iter()
                                .filter(|&&b| b == b'\n')
                                .count();
                            tab.pending_scroll_to_line = Some(line);
                        }
                    }
                }
            }
        }
        // 渲染内容
        match tab.active_view {
            ActiveView::Decompiled if tab.is_text || (tab.is_class && tab.is_source_unlocked()) => {
                if render::render_editable(ui, tab, &matches, current) {
                    self.action = Some(TabAction::RecompileSource(tab.entry_path.clone()));
                }
            }
            ActiveView::Decompiled => {
                let (hit, recompile) =
                    render::render_decompiled(ui, tab, &matches, current, self.known_classes);
                if let Some(hit) = hit {
                    self.pending_navigate = Some(PendingNavigate {
                        hit,
                        source_entry: tab.entry_path.clone(),
                        source_text: tab.decompiled.clone(),
                    });
                }
                if recompile {
                    self.action = Some(TabAction::RecompileSource(tab.entry_path.clone()));
                }
            }
            ActiveView::Bytecode => bytecode::render_bytecode_panel(ui, tab, &matches, current),
            ActiveView::Hex => render::render_hex(ui, tab, &matches, current),
        }
        // 浮动查找栏（overlay，用 clip_rect 定位到可见区域，不随 ScrollArea 滚动）
        if self.find_bar.open {
            let fbt = theme::find_bar_theme();
            self.find_bar.render_overlay(ui, ui.clip_rect(), &fbt);
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab, _path: NodePath) {
        let mt = &theme::menu_theme();
        if menu_item(
            ui,
            mt,
            &t!("editor.close"),
            Some(&keybindings::DEFAULT_CLOSE_TAB),
        ) {
            self.action = Some(TabAction::Close(tab.entry_path.clone()));
            ui.close();
        }
        if menu_item(ui, mt, &t!("editor.close_others"), None) {
            self.action = Some(TabAction::CloseOthers(tab.entry_path.clone()));
            ui.close();
        }
        if menu_item(ui, mt, &t!("editor.close_to_right"), None) {
            self.action = Some(TabAction::CloseToRight(tab.entry_path.clone()));
            ui.close();
        }
        if menu_item(
            ui,
            mt,
            &t!("editor.close_all"),
            Some(&keybindings::DEFAULT_CLOSE_ALL_TABS),
        ) {
            self.action = Some(TabAction::CloseAll);
            ui.close();
        }
    }

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        match &tab.entry_path {
            Some(path) => egui::Id::new(path),
            None => egui::Id::new(&tab.title),
        }
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        if tab.is_modified || tab.source_modified {
            self.pending_close = Some(tab.entry_path.clone());
            return OnCloseResponse::Focus;
        }
        OnCloseResponse::Close
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }

    fn modification_color(&self, tab: &Self::Tab) -> Option<egui::Color32> {
        if tab.is_modified || tab.source_modified {
            return Some(theme::ACCENT_ORANGE);
        }
        if let Some(path) = &tab.entry_path {
            if self.jar_modified.contains(path) {
                return Some(theme::ACCENT_GREEN);
            }
        }
        None
    }
}
