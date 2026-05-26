//! 字节码结构化面板：左侧成员导航 + 右侧详情/代码
//!
//! @author sky

mod detail;
mod nav;

use super::tab::{BytecodeSelection, EditorTab};
use crate::appearance::theme;
use detail::{render_class_info_editable, render_field_editable, render_method_editable};
use eframe::egui;
use egui_editor::search::FindMatch;

/// 渲染字节码结构化面板
pub fn render_bytecode_panel(
    ui: &mut egui::Ui,
    tab: &mut EditorTab,
    matches: &[FindMatch],
    current: Option<usize>,
) {
    let read_only = tab.is_source_unlocked();
    if tab.class_structure.is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("No class data")
                    .color(theme::TEXT_MUTED)
                    .size(14.0),
            );
        });
        return;
    }
    let rect = ui.max_rect();
    let nav_w = tab
        .nav_width
        .clamp(theme::BYTECODE_MIN_NAV_WIDTH, rect.width() - 100.0);
    tab.nav_width = nav_w;
    let painter = ui.painter();
    // 左侧导航背景
    let nav_rect = egui::Rect::from_min_size(rect.left_top(), egui::vec2(nav_w, rect.height()));
    painter.rect_filled(nav_rect, 0.0, theme::BG_GUTTER);
    let divider_x = rect.left() + nav_w;
    // 右侧内容背景
    let content_rect =
        egui::Rect::from_min_max(egui::pos2(divider_x, rect.top()), rect.right_bottom());
    painter.rect_filled(content_rect, 0.0, theme::BG_DARKEST);
    // 拖拽 resize 手柄
    let handle_rect = egui::Rect::from_min_size(
        egui::pos2(
            divider_x - theme::BYTECODE_RESIZE_HANDLE_W / 2.0,
            rect.top(),
        ),
        egui::vec2(theme::BYTECODE_RESIZE_HANDLE_W, rect.height()),
    );
    let handle_id = ui.id().with("bc_resize");
    let handle_resp = ui.interact(handle_rect, handle_id, egui::Sense::drag());
    if handle_resp.dragged() {
        tab.nav_width = (nav_w + handle_resp.drag_delta().x).clamp(
            theme::BYTECODE_MIN_NAV_WIDTH,
            theme::BYTECODE_MAX_NAV_WIDTH.min(rect.width() - 100.0),
        );
    }
    if handle_resp.hovered() || handle_resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
    }
    // 左侧导航（immutable borrow，scope 内释放）
    let selection = tab.bc_selection;
    let mut new_selection = None;
    {
        let cs = tab.class_structure.as_ref().unwrap();
        let field_count = cs.fields.len();
        let method_count = cs.methods.len();
        let mut nav_ui = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect));
        nav_ui.set_clip_rect(nav_rect);
        egui::ScrollArea::vertical()
            .id_salt(ui.id().with("bc_nav"))
            .auto_shrink(false)
            .show(&mut nav_ui, |ui| {
                ui.set_min_width(nav_w);
                new_selection = nav::render_nav(ui, cs, selection, field_count, method_count);
            });
    }
    if let Some(sel) = new_selection {
        tab.bc_selection = sel;
    }
    // 右侧内容（可 mutable borrow）
    let selection = tab.bc_selection;
    let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));
    let changed = match selection {
        BytecodeSelection::ClassInfo => {
            let cs = tab.class_structure.as_mut().unwrap();
            let c = render_class_info_editable(&mut content_ui, cs, read_only);
            if c {
                cs.info.modified = true;
            }
            c
        }
        BytecodeSelection::Field(idx) => {
            let cs = tab.class_structure.as_mut().unwrap();
            cs.fields.get_mut(idx).map_or(false, |field| {
                let c = render_field_editable(&mut content_ui, field, idx, read_only);
                if c {
                    field.modified = true;
                }
                c
            })
        }
        BytecodeSelection::Method(idx) => {
            let cs = tab.class_structure.as_mut().unwrap();
            let scroll = &mut tab.pending_scroll_to_line;
            cs.methods.get_mut(idx).map_or(false, |method| {
                let c = render_method_editable(
                    &mut content_ui,
                    method,
                    idx,
                    matches,
                    current,
                    scroll,
                    read_only,
                );
                if c {
                    method.modified = true;
                }
                c
            })
        }
    };
    if changed {
        tab.is_modified = true;
    }
}
