//! 输入处理：鼠标交互、hit test、右键菜单
//!
//! @author sky

use crate::layout::Cols;
use crate::{BYTES_PER_ROW, HEX_CHARS, HexTheme, HexViewState, PAD_TOP, ROW_H, Region};
use eframe::egui;
/// 鼠标位置 → 字节索引 + 区域
pub(crate) fn hit_test(
    pos: egui::Pos2,
    cols: &Cols,
    origin: egui::Pos2,
    data_len: usize,
    total_rows: usize,
) -> Option<(usize, Region)> {
    let rx = pos.x - origin.x;
    let ry = pos.y - origin.y - PAD_TOP;
    tabookit::ensure!(ry >= 0.0);
    let row = (ry / ROW_H) as usize;
    tabookit::ensure!(row < total_rows);
    let row_offset = row * BYTES_PER_ROW;
    let row_end = (row_offset + BYTES_PER_ROW).min(data_len);
    let row_count = row_end - row_offset;
    // hex 区域
    if rx >= cols.hex_x && rx < cols.hex_x + HEX_CHARS as f32 * cols.char_w + cols.group_gap {
        for col in 0..row_count {
            let hx = cols.hex_byte_x(col);
            if rx >= hx && rx < hx + cols.char_w * 2.5 {
                return Some((row_offset + col, Region::Hex));
            }
        }
        return None;
    }
    // ASCII 区域
    if rx >= cols.ascii_x {
        let col = ((rx - cols.ascii_x) / cols.char_w) as usize;
        if col < row_count {
            return Some((row_offset + col, Region::Ascii));
        }
    }
    None
}
pub(crate) fn handle_mouse(
    response: &egui::Response,
    cols: &Cols,
    origin: egui::Pos2,
    data: &[u8],
    state: &mut HexViewState,
    total_rows: usize,
) {
    if response.clicked() || response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((idx, region)) = hit_test(pos, cols, origin, data.len(), total_rows) {
                state.cursor = Some(idx);
                state.selection = None;
                state.drag_anchor = Some(idx);
                state.active_region = region;
            } else {
                state.cursor = None;
                state.selection = None;
                state.drag_anchor = None;
            }
        }
    }
    if response.dragged() {
        if let (Some(anchor), Some(pos)) = (state.drag_anchor, response.interact_pointer_pos()) {
            if let Some((idx, _)) = hit_test(pos, cols, origin, data.len(), total_rows) {
                let (start, end) = if idx < anchor {
                    (idx, anchor + 1)
                } else {
                    (anchor, idx + 1)
                };
                state.cursor = Some(idx);
                if end - start > 1 {
                    state.selection = Some((start, end));
                } else {
                    state.selection = None;
                }
            }
        }
    }
    if response.drag_stopped() {
        state.drag_anchor = None;
    }
}
/// 选区范围 [start, end)，无选区时返回 (0, 0)
fn selection_range(state: &HexViewState) -> (usize, usize) {
    tabookit::first!(it in
        state.selection => *it,
        state.cursor => (*it, *it + 1),
        else (0, 0),
    )
}

pub(crate) fn context_menu(
    ui: &mut egui::Ui,
    data: &[u8],
    state: &mut HexViewState,
    theme: &HexTheme,
) {
    let has_data = state.selection.is_some() || state.cursor.is_some();
    if ui
        .add_enabled(has_data, egui::Button::new(&theme.labels.copy_hex))
        .clicked()
    {
        let (start, end) = selection_range(state);
        if start < end {
            let text = data[start..end.min(data.len())]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            ui.ctx().copy_text(text);
        }
        ui.close();
    }
    if ui
        .add_enabled(has_data, egui::Button::new(&theme.labels.copy_ascii))
        .clicked()
    {
        let (start, end) = selection_range(state);
        if start < end {
            let text: String = data[start..end.min(data.len())]
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            ui.ctx().copy_text(text);
        }
        ui.close();
    }
    ui.separator();
    if ui
        .add_enabled(
            state.cursor.is_some(),
            egui::Button::new(&theme.labels.copy_offset),
        )
        .clicked()
    {
        if let Some(c) = state.cursor {
            ui.ctx().copy_text(format!("0x{c:08X}"));
        }
        ui.close();
    }
    ui.separator();
    if ui.button(&theme.labels.select_all).clicked() {
        state.cursor = Some(0);
        state.selection = Some((0, data.len()));
        ui.close();
    }
}
