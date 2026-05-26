//! 代码视图共享 frame 流程
//!
//! @author sky

use super::gutter::{paint_line_numbers, GUTTER_PAD, TEXT_PAD_LEFT};
use super::layout::{extract_highlight_word, paint_word_highlight_overlay};
use super::scroll::{apply_scroll_delta, detect_edge_scroll};
use super::text_edit::code_text_edit;
use crate::theme::CodeViewTheme;
use eframe::egui;
use std::sync::Arc;

/// 行高亮动画保持时间（秒）
const LINE_HIGHLIGHT_HOLD: f64 = 0.5;
/// 行高亮动画淡出时间（秒）
const LINE_HIGHLIGHT_FADE: f64 = 0.8;

pub(crate) struct TextEditFrameState {
    pub(crate) galley_y: f32,
    pub(crate) edge_scroll_delta: egui::Vec2,
    pub(crate) wheel_delta: egui::Vec2,
    pub(crate) cursor_primary: usize,
    pub(crate) cursor_secondary: usize,
    pub(crate) galley: Option<Arc<egui::Galley>>,
    pub(crate) galley_pos: egui::Pos2,
    pub(crate) response: Option<egui::Response>,
}

fn paint_line_highlight_animation(
    ui: &egui::Ui,
    hl_time_id: egui::Id,
    hl_line_id: egui::Id,
    code_font: &egui::FontId,
    theme: &CodeViewTheme,
) {
    let hl_time: Option<f64> = ui.ctx().data(|d| d.get_temp(hl_time_id));
    let Some(start) = hl_time else {
        return;
    };
    let elapsed = ui.input(|i| i.time) - start;
    let alpha = if elapsed < LINE_HIGHLIGHT_HOLD {
        1.0
    } else {
        (1.0 - ((elapsed - LINE_HIGHLIGHT_HOLD) / LINE_HIGHLIGHT_FADE)).max(0.0)
    };
    if alpha <= 0.001 {
        ui.ctx().data_mut(|d| {
            d.remove_temp::<f64>(hl_time_id);
            d.remove_temp::<usize>(hl_line_id);
        });
        return;
    }
    let hl_line: usize = ui.ctx().data(|d| d.get_temp(hl_line_id).unwrap_or(0));
    let row_h = ui.fonts_mut(|f| {
        f.layout_no_wrap("M".into(), code_font.clone(), egui::Color32::WHITE)
            .size()
            .y
    });
    let top_y = ui.min_rect().top();
    let y = top_y + hl_line as f32 * row_h;
    let full_width = ui.max_rect().width();
    let rect = egui::Rect::from_min_size(
        egui::pos2(ui.max_rect().left(), y),
        egui::vec2(full_width, row_h),
    );
    let [r, g, b, _] = theme.line_highlight.to_array();
    let a = (alpha as f32 * 255.0) as u8;
    ui.painter()
        .rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
    ui.ctx().request_repaint_after_secs(0.016);
}

pub(crate) fn remember_line_highlight(
    ui: &mut egui::Ui,
    hl_time_id: egui::Id,
    hl_line_id: egui::Id,
    scroll_to_line: Option<usize>,
) {
    let Some(line) = scroll_to_line else {
        return;
    };
    let now = ui.input(|i| i.time);
    ui.ctx().data_mut(|d| {
        d.insert_temp(hl_time_id, now);
        d.insert_temp(hl_line_id, line);
    });
}

pub(crate) fn line_highlight_ids(id: egui::Id) -> (egui::Id, egui::Id) {
    (id.with("__hl_time"), id.with("__hl_line"))
}

pub(crate) fn show_code_view_frame(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    id: egui::Id,
    code_font: &egui::FontId,
    theme: &CodeViewTheme,
    gutter_w: f32,
    hl_time_id: egui::Id,
    hl_line_id: egui::Id,
    line_count: usize,
    scroll_to_line: &mut Option<usize>,
    layouter: &mut dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>,
    mut inspect_output: impl FnMut(&egui::Ui, &egui::text_edit::TextEditOutput),
) -> TextEditFrameState {
    let mut frame = TextEditFrameState {
        galley_y: 0.0,
        edge_scroll_delta: egui::Vec2::ZERO,
        wheel_delta: egui::Vec2::ZERO,
        cursor_primary: 0,
        cursor_secondary: 0,
        galley: None,
        galley_pos: egui::Pos2::ZERO,
        response: None,
    };
    ui.horizontal_top(|ui| {
        ui.add_space(gutter_w + GUTTER_PAD + TEXT_PAD_LEFT);
        paint_line_highlight_animation(ui, hl_time_id, hl_line_id, code_font, theme);
        let output = code_text_edit(text, id, code_font.clone(), layouter).show(ui);
        frame.galley_y = output.galley_pos.y;
        frame.galley = Some(output.galley.clone());
        frame.galley_pos = output.galley_pos;
        if let Some(cr) = output.cursor_range.as_ref() {
            frame.cursor_primary = cr.primary.index;
            frame.cursor_secondary = cr.secondary.index;
        }
        inspect_output(ui, &output);
        frame.response = Some(output.response.response.clone());
        scroll_to_requested_line(ui, &output, frame.galley_y, line_count, scroll_to_line);
        if output.response.dragged() {
            let (ed, wd) = detect_edge_scroll(&output.response, ui);
            frame.edge_scroll_delta = ed;
            frame.wheel_delta = wd;
        }
    });
    frame
}

fn scroll_to_requested_line(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    galley_y: f32,
    line_count: usize,
    scroll_to_line: &mut Option<usize>,
) {
    let Some(line) = scroll_to_line.take() else {
        return;
    };
    let row_h = output.galley.size().y / line_count.max(1) as f32;
    let target_y = galley_y + line as f32 * row_h;
    let line_rect = egui::Rect::from_min_size(
        egui::pos2(output.galley_pos.x, target_y),
        egui::vec2(1.0, row_h),
    );
    ui.scroll_to_rect(line_rect, Some(egui::Align::Center));
}

pub(crate) fn finish_code_view_frame(
    ui: &mut egui::Ui,
    text: &str,
    frame: &TextEditFrameState,
    prev_word_ranges: &[(usize, usize)],
    line_count: usize,
    line_mapping: &[Option<u32>],
    gutter_w: f32,
    code_font: &egui::FontId,
    theme: &CodeViewTheme,
) -> Option<String> {
    if let Some(g) = frame.galley.as_ref() {
        paint_word_highlight_overlay(
            ui,
            g,
            frame.galley_pos,
            text,
            prev_word_ranges,
            frame.cursor_primary,
            frame.cursor_secondary,
            theme.word_highlight_bg,
        );
    }
    let new_word = extract_highlight_word(text, frame.cursor_primary, frame.cursor_secondary);
    apply_scroll_delta(ui, frame.edge_scroll_delta, frame.wheel_delta);
    paint_line_numbers(
        ui,
        frame.galley_y,
        line_count,
        line_mapping,
        gutter_w,
        code_font,
        theme,
    );
    new_word
}
