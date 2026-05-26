//! 未保存变更确认：业务逻辑 + 动作分发
//!
//! UI 渲染委托给 `egui_shell::components::ConfirmDialog`。
//!
//! @author sky

use super::editor::TabAction;
use crate::app::App;
use crate::appearance::theme;
use crate::settings::OpenBehavior;
use eframe::egui;
use egui_shell::components::{ConfirmDialog, ConfirmResult, FlatButton};
use rust_i18n::t;
use std::path::PathBuf;

/// 需要用户确认的破坏性动作
pub enum ConfirmAction {
    /// 关闭整个应用窗口
    Close,
    /// 打开文件选择器加载新 JAR
    OpenDialog,
    /// 打开指定 JAR 文件
    Open(PathBuf),
    /// 关闭 tab（单个/批量）
    TabClose(TabAction),
    /// 大 JAR 全量反编译确认
    DecompileAll,
    /// 选择在哪个窗口打开 JAR（Ask 模式）
    OpenWindowChoice(PathBuf),
}

impl App {
    /// 是否有未保存的变更（tab 级别编辑 或 JAR 内存级别修改）
    pub fn has_unsaved_changes(&self) -> bool {
        self.workspace
            .jar()
            .is_some_and(|j| j.has_modified_entries())
            || self
                .layout
                .editor
                .dock_state
                .iter_all_tabs()
                .any(|(_, tab)| tab.is_modified || tab.source_modified)
    }

    /// 带确认的关闭请求
    pub fn request_close(&mut self, ctx: &egui::Context) {
        if self.has_unsaved_changes() {
            self.pending_confirm = Some(ConfirmAction::Close);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// 带确认的打开 JAR 对话框
    pub fn request_open_jar_dialog(&mut self) {
        if !self.workspace.is_loaded() {
            self.open_jar_dialog();
            return;
        }
        match self.settings.open_behavior {
            OpenBehavior::CurrentWindow => {
                if self.has_unsaved_changes() {
                    self.pending_confirm = Some(ConfirmAction::OpenDialog);
                } else {
                    self.open_jar_dialog();
                }
            }
            OpenBehavior::NewWindow => {
                if let Some((path, is_jar)) = Self::pick_file() {
                    if is_jar {
                        self.spawn_new_window(&path);
                    } else {
                        self.open_standalone_file(&path);
                    }
                }
            }
            OpenBehavior::Ask => {
                if let Some((path, is_jar)) = Self::pick_file() {
                    if is_jar {
                        self.pending_confirm = Some(ConfirmAction::OpenWindowChoice(path));
                    } else {
                        self.open_standalone_file(&path);
                    }
                }
            }
        }
    }

    /// 带确认的打开指定 JAR
    pub fn request_open_jar(&mut self, path: &std::path::Path) {
        if !self.workspace.is_loaded() {
            self.open_jar(path);
            return;
        }
        match self.settings.open_behavior {
            OpenBehavior::CurrentWindow => {
                if self.has_unsaved_changes() {
                    self.pending_confirm = Some(ConfirmAction::Open(path.to_path_buf()));
                } else {
                    self.open_jar(path);
                }
            }
            OpenBehavior::NewWindow => {
                self.spawn_new_window(path);
            }
            OpenBehavior::Ask => {
                self.pending_confirm = Some(ConfirmAction::OpenWindowChoice(path.to_path_buf()));
            }
        }
    }

    /// 执行已确认的动作
    fn execute_confirmed(&mut self, action: ConfirmAction, ctx: &egui::Context) {
        match action {
            ConfirmAction::Close | ConfirmAction::OpenDialog | ConfirmAction::Open(_) => {
                // 全局动作：清除所有未保存标记，避免 has_unsaved_changes() 再次拦截
                for (_, tab) in self.layout.editor.dock_state.iter_all_tabs_mut() {
                    tab.is_modified = false;
                    tab.source_modified = false;
                }
                if let Some(jar) = self.workspace.jar_mut() {
                    jar.clear_modified();
                }
                match action {
                    ConfirmAction::Close => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ConfirmAction::OpenDialog => {
                        self.open_jar_dialog();
                    }
                    ConfirmAction::Open(path) => {
                        self.open_jar(&path);
                    }
                    _ => unreachable!(),
                }
            }
            ConfirmAction::TabClose(tab_action) => {
                self.layout.editor.force_tab_action(tab_action);
            }
            ConfirmAction::DecompileAll => {
                self.start_confirmed_decompile();
            }
            ConfirmAction::OpenWindowChoice(_) => {
                // 由独立的多选项弹窗处理，不经过此分支
            }
        }
    }

    /// 渲染确认弹窗（根据 ConfirmAction 类型选择标题/按钮）
    pub(crate) fn render_confirm(&mut self, ctx: &egui::Context) {
        let Some(action) = &self.pending_confirm else {
            return;
        };
        if matches!(action, ConfirmAction::OpenWindowChoice(_)) {
            self.render_window_choice(ctx);
            return;
        }
        let is_decompile = matches!(action, ConfirmAction::DecompileAll);
        let (title, message, confirm_label, cancel_label, confirm_color);
        if is_decompile {
            let (size_mb, count) = self
                .workspace
                .jar()
                .map(|j| (j.file_size as f64 / 1_000_000.0, j.class_count()))
                .unwrap_or((0.0, 0));
            let size_str = if size_mb >= 10.0 {
                format!("{size_mb:.0}")
            } else {
                format!("{size_mb:.1}")
            };
            title = t!("confirm.decompile_title");
            message = t!("confirm.decompile_message", size = size_str, count = count);
            confirm_label = t!("confirm.decompile_yes");
            cancel_label = t!("confirm.decompile_skip");
            confirm_color = None;
        } else {
            title = t!("confirm.unsaved_title");
            message = t!("confirm.unsaved_message");
            confirm_label = t!("confirm.discard");
            cancel_label = t!("confirm.cancel");
            confirm_color = Some(theme::ACCENT_RED);
        };
        let mut dialog = ConfirmDialog::new(&title, &message)
            .confirm_label(&confirm_label)
            .cancel_label(&cancel_label)
            .close_on_click_outside(!is_decompile);
        if let Some(color) = confirm_color {
            dialog = dialog.confirm_color(color);
        }
        let result = dialog.show(ctx, &theme::confirm_theme());
        match result {
            ConfirmResult::Confirmed => {
                let action = self.pending_confirm.take().unwrap();
                self.execute_confirmed(action, ctx);
            }
            ConfirmResult::Dismissed => {
                self.pending_confirm = None;
            }
            ConfirmResult::None => {}
        }
    }

    /// 渲染窗口选择弹窗（当前窗口 / 新窗口 / 取消）
    fn render_window_choice(&mut self, ctx: &egui::Context) {
        let ct = theme::confirm_theme();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.pending_confirm = None;
            return;
        }
        // 遮罩
        let backdrop_layer =
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("confirm_backdrop"));
        ctx.layer_painter(backdrop_layer)
            .rect_filled(ctx.content_rect(), 0.0, ct.backdrop);
        // 弹窗内容 + 按钮
        let mut choice: Option<u8> = None;
        let dialog_rect = egui::Area::new(egui::Id::new("window_choice_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ct.frame.show(ui, |ui| {
                    ui.set_width(380.0);
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 20,
                            right: 20,
                            top: 20,
                            bottom: 16,
                        })
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(t!("confirm.window_choice_title"))
                                    .size(13.0)
                                    .color(ct.title_color)
                                    .strong(),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(t!("confirm.window_choice_message"))
                                    .size(12.0)
                                    .color(ct.message_color),
                            );
                        });
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 20,
                            right: 20,
                            top: 12,
                            bottom: 12,
                        })
                        .show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let cancel_text = t!("confirm.cancel");
                                    let cancel = FlatButton::new(&cancel_text, &ct.button)
                                        .min_size(egui::vec2(72.0, 28.0));
                                    if ui.add(cancel).clicked() {
                                        choice = Some(0);
                                    }
                                    ui.add_space(6.0);
                                    let new_text = t!("confirm.window_new");
                                    let new_win = FlatButton::new(&new_text, &ct.button)
                                        .min_size(egui::vec2(88.0, 28.0));
                                    if ui.add(new_win).clicked() {
                                        choice = Some(2);
                                    }
                                    ui.add_space(6.0);
                                    let current_text = t!("confirm.window_current");
                                    let current_win = FlatButton::new(&current_text, &ct.button)
                                        .min_size(egui::vec2(88.0, 28.0));
                                    if ui.add(current_win).clicked() {
                                        choice = Some(1);
                                    }
                                },
                            );
                        });
                });
            })
            .response
            .rect;
        // 点击外部关闭
        if choice.is_none() && ctx.input(|i| i.pointer.any_pressed()) {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                if !dialog_rect.contains(pos) {
                    choice = Some(0);
                }
            }
        }
        match choice {
            // 取消
            Some(0) => {
                self.pending_confirm = None;
            }
            // 当前窗口
            Some(1) => {
                let path = match self.pending_confirm.take() {
                    Some(ConfirmAction::OpenWindowChoice(p)) => p,
                    _ => unreachable!(),
                };
                if self.has_unsaved_changes() {
                    self.pending_confirm = Some(ConfirmAction::Open(path));
                } else {
                    self.open_jar(&path);
                }
            }
            // 新窗口
            Some(2) => {
                let path = match self.pending_confirm.take() {
                    Some(ConfirmAction::OpenWindowChoice(p)) => p,
                    _ => unreachable!(),
                };
                self.spawn_new_window(&path);
            }
            _ => {}
        }
    }

}
