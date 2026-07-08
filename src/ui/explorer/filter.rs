//! Explorer 速搜过滤：键盘输入捕获、后台匹配、浮层渲染
//!
//! @author sky

use super::FilePanel;
use super::tree;
use crate::appearance::{codicon, theme};
use crate::task::{Poll, Pollable, Task};
use eframe::egui;
use std::sync::Arc;

impl FilePanel {
    /// 更新面板隐式焦点：点击面板内获得，点击面板外或有 widget 聚焦时失去
    pub(super) fn update_focus(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        // 有文本控件聚焦时（如编辑器 TextEdit、查找栏）让出焦点
        if ctx.memory(|m| m.focused().is_some()) {
            if self.focused {
                self.focused = false;
                self.clear_filter();
            }
            return;
        }
        // 检测主键点击位置
        if ctx.input(|i| i.pointer.primary_clicked()) {
            let inside = ctx
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| rect.contains(p));
            if inside {
                self.focused = true;
            } else {
                self.focused = false;
                self.clear_filter();
            }
        }
    }

    /// 捕获键盘输入用于速搜过滤（仅在面板拥有焦点时调用）
    pub(super) fn capture_input(&mut self, ctx: &egui::Context) {
        if self.roots.is_empty() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut changed = false;
        for event in &events {
            match event {
                egui::Event::Text(text) => {
                    // 忽略组合键产生的文本事件（Alt+1、Ctrl+O 等）
                    let has_modifier = ctx.input(|i| {
                        let m = i.modifiers;
                        m.alt || m.ctrl || m.command
                    });
                    if !has_modifier {
                        self.filter.push_str(text);
                        changed = true;
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.any() && !self.filter.is_empty() => {
                    self.filter.pop();
                    changed = true;
                }
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } if !self.filter.is_empty() => {
                    self.clear_filter();
                }
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                } if !self.filter.is_empty() => {
                    if let Some(path) = self.selected.clone() {
                        tree::reveal(&mut self.roots, &path);
                        self.pending_open = Some(path);
                        self.scroll_to_selected = true;
                    }
                    self.clear_filter();
                }
                _ => {}
            }
        }
        if changed {
            self.dispatch_filter();
        }
    }

    /// 发起后台过滤计算
    fn dispatch_filter(&mut self) {
        self.filter_gen += 1;
        if self.filter.is_empty() {
            self.filter_visible.clear();
            return;
        }
        // 首次过滤时构建索引
        if self.filter_index.is_none() {
            self.filter_index = Some(Arc::new(tree::build_filter_index(&self.roots)));
        }
        let lower = self.filter.to_ascii_lowercase();
        let index = Arc::clone(self.filter_index.as_ref().unwrap());
        let seq = self.filter_gen;
        self.filter_task = Some(Task::spawn(move || {
            let result = tree::compute_filter(&index, &lower);
            (seq, result)
        }));
    }

    /// 拉取后台过滤结果（每帧调用，非阻塞）
    pub(super) fn poll_filter_result(&mut self) {
        let task = tabookit::or!(&self.filter_task, return);
        let (seq, result) = match task.poll() {
            Poll::Ready(v) => v,
            Poll::Pending => return,
            Poll::Lost => {
                self.filter_task = None;
                return;
            }
        };
        self.filter_task = None;
        // 丢弃过期结果
        if seq != self.filter_gen {
            return;
        }
        self.filter_visible = result.visible;
        if result.first_match.is_some() {
            self.selected = result.first_match;
            self.scroll_to_selected = true;
        }
    }

    /// 清除过滤状态
    pub(super) fn clear_filter(&mut self) {
        self.filter.clear();
        self.filter_visible.clear();
        self.filter_gen += 1;
    }

    /// 在面板底部绘制过滤条浮层
    pub(super) fn render_filter_bar(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self.filter.is_empty() {
            return;
        }
        let painter = ui.painter();
        let bar_h = 24.0;
        let m = 6.0;
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + m, rect.bottom() - bar_h - m),
            egui::pos2(rect.right() - m, rect.bottom() - m),
        );
        painter.rect_filled(bar_rect, 4.0, theme::BG_MEDIUM);
        painter.rect_stroke(
            bar_rect,
            4.0,
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Middle,
        );
        // 搜索图标
        painter.text(
            egui::pos2(bar_rect.left() + 10.0, bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            codicon::SEARCH,
            egui::FontId::new(11.0, codicon::family()),
            theme::TEXT_MUTED,
        );
        // 过滤文本
        painter.text(
            egui::pos2(bar_rect.left() + 26.0, bar_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &self.filter,
            egui::FontId::proportional(12.0),
            theme::TEXT_PRIMARY,
        );
    }
}
