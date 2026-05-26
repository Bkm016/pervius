//! 编辑器视图渲染：代码视图 + Hex 视图
//!
//! 代码视图委托给 egui-editor，此处提供 EditorTab 解包的薄包装。
//!
//! @author sky

use super::tab::EditorTab;
use crate::appearance::theme;
use eframe::egui;
use egui_editor::code_view::NavigationHit;
use egui_editor::search::FindMatch;
use egui_shell::components::menu_item;
use rust_i18n::t;
use std::collections::HashSet;

pub use egui_editor::code_view::line_number_width;

/// 在 ScrollArea 外绘制全高背景（左侧 gutter + 右侧编辑区）
pub fn paint_editor_bg(ui: &egui::Ui, full_rect: egui::Rect, gutter_w: f32) {
    let t = theme::editor_theme();
    egui_editor::code_view::paint_editor_bg(ui, full_rect, gutter_w, &t);
}

/// 反编译视图
///
/// 返回 Ctrl+Click 产生的导航请求（如有）
pub fn render_decompiled(
    ui: &mut egui::Ui,
    tab: &mut EditorTab,
    matches: &[FindMatch],
    current: Option<usize>,
    known_classes: &HashSet<String>,
) -> (Option<NavigationHit>, bool) {
    let t = theme::editor_theme();
    let output = egui_editor::code_view::code_view(
        ui,
        egui::Id::new(&tab.entry_path).with("decompiled"),
        &tab.decompiled,
        &tab.decompiled_data.spans,
        &tab.decompiled_line_mapping,
        matches,
        current,
        &t,
        &mut tab.layout_cache,
        &mut tab.pending_scroll_to_line,
        Some(known_classes),
    );
    let recompile = show_source_context_menu(output.response.as_ref(), tab);
    paint_source_edit_indicator(ui, tab);
    paint_compile_diagnostics(ui, tab);
    (output.value, recompile)
}

/// 可编辑文本视图
///
/// 文本变更后自动刷新语法高亮数据并标记 tab 为已修改。
pub fn render_editable(
    ui: &mut egui::Ui,
    tab: &mut EditorTab,
    matches: &[FindMatch],
    current: Option<usize>,
) -> bool {
    let t = theme::editor_theme();
    let output = egui_editor::code_view::code_view_editable(
        ui,
        egui::Id::new(&tab.entry_path).with("editable"),
        &mut tab.decompiled,
        tab.language,
        matches,
        current,
        &t,
        &mut tab.editable_layout_cache,
        tab.viewport_override,
        &mut tab.pending_scroll_to_line,
    );
    if output.value {
        tab.refresh_decompiled_data();
        if tab.is_class && tab.is_source_unlocked() {
            tab.source_modified = true;
            tab.compile_diagnostics.clear();
        } else {
            tab.is_modified = true;
        }
    }
    let recompile = show_source_context_menu(output.response.as_ref(), tab);
    paint_source_edit_indicator(ui, tab);
    paint_compile_diagnostics(ui, tab);
    recompile
}

fn show_source_context_menu(response: Option<&egui::Response>, tab: &mut EditorTab) -> bool {
    if !tab.is_class {
        return false;
    }
    let Some(response) = response else {
        return false;
    };
    let mut recompile = false;
    response.context_menu(|ui| {
        let mt = &theme::menu_theme();
        if tab.is_class && !tab.is_source_unlocked() {
            let jdk_available = pervius_java_bridge::compiler::is_jdk_available();
            if jdk_available && !tab.is_modified {
                if menu_item(ui, mt, &t!("editor.allow_edit"), None) {
                    tab.unlock_source_edit();
                    ui.close();
                }
            } else {
                let label = if tab.is_modified {
                    t!("editor.source_vs_struct_conflict").to_string()
                } else {
                    t!("editor.jdk_required").to_string()
                };
                ui.add_enabled(false, egui::Button::new(label));
            }
        }
        if tab.is_class && tab.is_source_unlocked() {
            if menu_item(ui, mt, &t!("editor.lock_edit"), None) {
                tab.lock_source_edit();
                ui.close();
            }
            if tab.source_modified && menu_item(ui, mt, &t!("editor.discard_source"), None) {
                tab.discard_source_changes();
                ui.close();
            }
            if menu_item(ui, mt, &t!("editor.recompile_now"), None) {
                recompile = true;
                ui.close();
            }
        }
    });
    recompile
}

fn paint_source_edit_indicator(ui: &egui::Ui, tab: &EditorTab) {
    if !(tab.is_class && tab.is_source_unlocked()) {
        return;
    }
    let rect = ui.clip_rect();
    let x = rect.left() + 2.0;
    ui.painter().vline(
        x,
        rect.y_range(),
        egui::Stroke::new(1.0, theme::ACCENT_CYAN),
    );
}

fn paint_compile_diagnostics(ui: &egui::Ui, tab: &EditorTab) {
    if tab.compile_diagnostics.is_empty() {
        return;
    }
    let t = theme::editor_theme();
    let row_h = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            "M".to_string(),
            egui::FontId::monospace(t.code_font_size),
            egui::Color32::WHITE,
        )
        .size()
        .y
    });
    let clip = ui.clip_rect();
    let painter = ui.painter();
    for diag in &tab.compile_diagnostics {
        if diag.line == 0 {
            continue;
        }
        let y = ui.min_rect().top() + (diag.line.saturating_sub(1) as f32) * row_h + row_h * 0.5;
        if y < clip.top() || y > clip.bottom() {
            continue;
        }
        let pos = egui::pos2(clip.left() + 8.0, y);
        painter.circle_filled(pos, 3.0, theme::ACCENT_RED);
    }
}

/// Hex 视图
pub fn render_hex(
    ui: &mut egui::Ui,
    tab: &mut EditorTab,
    matches: &[FindMatch],
    current: Option<usize>,
) {
    let hex_theme = super::style::hex::hex_theme();
    let highlights: Vec<(usize, usize)> = matches.iter().map(|m| (m.start, m.end)).collect();
    egui_hex_view::show(
        ui,
        &tab.raw_bytes,
        &mut tab.hex_state,
        &hex_theme,
        &highlights,
        current,
    );
}
