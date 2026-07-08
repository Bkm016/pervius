//! 大文本视窗编辑模式
//!
//! 当文本超过阈值（100KB）时启用。
//! 仅将光标附近 ~6000 字符的窗口暴露给 TextEdit，
//! 将布局/渲染/交互开销从 O(n) 降至 O(窗口大小)。
//!
//! @author sky

use crate::code_view::{
    EditableLayoutCache, GUTTER_PAD, TEXT_PAD_LEFT, apply_scroll_delta, byte_offset_at_char,
    code_text_edit, detect_edge_scroll, extract_highlight_word, hash_text, line_number_width,
    paint_line_numbers, paint_word_highlight_overlay, rebuild_galley,
};
use crate::highlight::Language;
use crate::search::FindMatch;
use crate::theme::CodeViewTheme;
use eframe::egui;
use std::ops::Range;
use std::sync::Arc;

/// 视窗模式触发阈值（字节）
pub const VIEWPORT_TEXT_LEN: usize = 100_000;
/// 视窗半径（字符数）
const VIEWPORT_HALF_WINDOW: usize = 3000;
/// 触发窗口重新居中的边距（字符数）
const VIEWPORT_EDGE_MARGIN: usize = 200;

/// 计算视窗窗口范围（字符偏移）
///
/// 窗口尽量稳定不跳动，光标靠近边缘时才重新居中。
fn calculate_viewport_window(
    cursor_char: usize,
    total_chars: usize,
    prev_start: Option<usize>,
    half_window: usize,
) -> (usize, usize) {
    let window_size = half_window * 2;
    // 光标在已有窗口的舒适区内：保持不变
    if let Some(prev) = prev_start {
        let prev_end = (prev + window_size).min(total_chars);
        if cursor_char >= prev + VIEWPORT_EDGE_MARGIN
            && cursor_char + VIEWPORT_EDGE_MARGIN <= prev_end
        {
            return (prev, prev_end);
        }
    }
    // 重新居中
    let start = cursor_char.saturating_sub(half_window);
    let end = (start + window_size).min(total_chars);
    let start = if end.saturating_sub(start) < window_size && total_chars >= window_size {
        end.saturating_sub(window_size)
    } else {
        start
    };
    (start, end)
}

/// 大文本视窗编辑缓冲区
///
/// 将全量文本的一个字符窗口暴露给 TextEdit，
/// 所有编辑操作在内部自动转换为全局偏移。
struct ViewportTextBuffer<'a> {
    /// 完整文本
    full_text: &'a mut String,
    /// 窗口起始字节偏移
    window_start_byte: usize,
    /// 窗口结束字节偏移（不含）
    window_end_byte: usize,
}

/// 大文本视窗只读缓冲区
struct ReadonlyViewportTextBuffer<'a> {
    /// 完整文本
    full_text: &'a str,
    /// 窗口起始字节偏移
    window_start_byte: usize,
    /// 窗口结束字节偏移（不含）
    window_end_byte: usize,
}

impl egui::TextBuffer for ViewportTextBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.full_text[self.window_start_byte..self.window_end_byte]
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let window_str = &self.full_text[self.window_start_byte..self.window_end_byte];
        let local_byte = byte_offset_at_char(window_str, char_index);
        let global_byte = self.window_start_byte + local_byte;
        self.full_text.insert_str(global_byte, text);
        self.window_end_byte += text.len();
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<usize>) {
        let window_str = &self.full_text[self.window_start_byte..self.window_end_byte];
        let start = self.window_start_byte + byte_offset_at_char(window_str, char_range.start);
        let end = self.window_start_byte + byte_offset_at_char(window_str, char_range.end);
        self.full_text.drain(start..end);
        self.window_end_byte -= end - start;
    }

    fn clear(&mut self) {
        self.full_text
            .drain(self.window_start_byte..self.window_end_byte);
        self.window_end_byte = self.window_start_byte;
    }

    fn replace_with(&mut self, text: &str) {
        self.full_text
            .replace_range(self.window_start_byte..self.window_end_byte, text);
        self.window_end_byte = self.window_start_byte + text.len();
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<()>()
    }
}

impl egui::TextBuffer for ReadonlyViewportTextBuffer<'_> {
    fn is_mutable(&self) -> bool {
        false
    }

    fn as_str(&self) -> &str {
        &self.full_text[self.window_start_byte..self.window_end_byte]
    }

    fn insert_text(&mut self, _text: &str, _char_index: usize) -> usize {
        0
    }

    fn delete_char_range(&mut self, _char_range: Range<usize>) {}

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<()>()
    }
}

/// 视窗模式可编辑代码视图
///
/// 仅布局光标附近的文本窗口，将大文本的布局/渲染/交互开销从 O(n) 降至 O(窗口大小)。
pub(crate) fn code_view_editable_viewport(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    lang: Language,
    matches: &[FindMatch],
    current: Option<usize>,
    theme: &CodeViewTheme,
    cache: &mut Option<EditableLayoutCache>,
    _scroll_to_line: &mut Option<usize>,
    editable: bool,
) -> bool {
    let code_font = egui::FontId::monospace(theme.code_font_size);
    // 全文统计信息缓存：仅在文本长度变更时重新计算（避免每帧 O(n)）
    let text_len = text.len();
    let (total_chars, total_line_count) = match cache.as_ref() {
        Some(c) if c.full_text_len == text_len => (c.full_text_chars, c.full_text_lines),
        _ => {
            let chars = text.chars().count();
            let lines = text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;
            if let Some(c) = cache.as_mut() {
                c.full_text_chars = chars;
                c.full_text_lines = lines;
                c.full_text_len = text_len;
            }
            (chars, lines)
        }
    };
    let gutter_w = line_number_width(total_line_count);

    // 上一帧的光标位置和窗口起点
    let cursor_char = cache.as_ref().map_or(0, |c| c.viewport_cursor_char);
    let prev_window_start = cache.as_ref().map(|c| c.viewport_window_start);
    let (win_start_char, win_end_char) = calculate_viewport_window(
        cursor_char,
        total_chars,
        prev_window_start,
        VIEWPORT_HALF_WINDOW,
    );

    // 视窗模式禁用 undo：每帧清空 undoer
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }

    // 字符偏移转字节偏移（创建 viewport_buf 之前完成，之后 text 被借用）
    let win_start_byte = byte_offset_at_char(text, win_start_char);
    let win_end_byte = byte_offset_at_char(text, win_end_char);

    // 搜索匹配映射到窗口本地字节偏移
    let local_matches: Vec<(usize, usize)> = matches
        .iter()
        .filter_map(|m| {
            if m.end <= win_start_byte || m.start >= win_end_byte {
                return None;
            }
            Some((
                m.start.saturating_sub(win_start_byte),
                m.end.min(win_end_byte) - win_start_byte,
            ))
        })
        .collect();
    let match_ref = local_matches.as_slice();
    // 定位当前高亮匹配在本地列表中的索引
    let local_current = current.and_then(|ci| {
        let m = matches.get(ci)?;
        if m.end <= win_start_byte || m.start >= win_end_byte {
            return None;
        }
        let ls = m.start.saturating_sub(win_start_byte);
        let le = m.end.min(win_end_byte) - win_start_byte;
        local_matches.iter().position(|lm| lm.0 == ls && lm.1 == le)
    });

    // 选中同名高亮映射到窗口本地字节偏移
    let local_word_ranges: Vec<(usize, usize)> = cache
        .as_ref()
        .map(|c| &c.word_highlight_ranges)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|&(s, e)| {
            if e <= win_start_byte || s >= win_end_byte {
                return None;
            }
            Some((
                s.saturating_sub(win_start_byte),
                e.min(win_end_byte) - win_start_byte,
            ))
        })
        .collect();
    let word_ref = local_word_ranges.as_slice();
    let word_gen = cache.as_ref().map_or(0, |c| c.word_gen);

    // layouter: 窗口文本很小（~6KB），通过 rebuild_galley 全量布局
    let mut layouter =
        |ui: &egui::Ui, text_buf: &dyn egui::TextBuffer, _wrap_width: f32| -> Arc<egui::Galley> {
            let s = text_buf.as_str();
            let th = hash_text(s);
            rebuild_galley(
                ui,
                s,
                th,
                lang,
                match_ref,
                local_current,
                word_ref,
                word_gen,
                theme,
                cache,
                true,
                cursor_char,
                win_start_char,
            )
            .unwrap()
        };

    let mut galley_y = 0.0f32;
    let mut edge_scroll_delta = egui::Vec2::ZERO;
    let mut wheel_delta = egui::Vec2::ZERO;
    let mut changed = false;
    let mut new_cursor_char = cursor_char;
    let mut selection_secondary_global = cursor_char;
    let mut out_galley: Option<std::sync::Arc<egui::Galley>> = None;
    let mut out_galley_pos = egui::Pos2::ZERO;

    let mut text_len_after_edit = text.len();
    let win_text_start = win_start_byte;
    let mut win_text_end = win_end_byte;
    if editable {
        let mut viewport_buf = ViewportTextBuffer {
            full_text: text,
            window_start_byte: win_start_byte,
            window_end_byte: win_end_byte,
        };
        ui.horizontal_top(|ui| {
            ui.add_space(gutter_w + GUTTER_PAD + TEXT_PAD_LEFT);
            let output =
                code_text_edit(&mut viewport_buf, id, code_font.clone(), &mut layouter).show(ui);
            galley_y = output.galley_pos.y;
            out_galley = Some(output.galley.clone());
            out_galley_pos = output.galley_pos;
            changed = output.response.changed();
            if let Some(cr) = output.cursor_range {
                new_cursor_char = cr.primary.index + win_start_char;
                selection_secondary_global = cr.secondary.index + win_start_char;
            }
            if output.response.dragged() {
                let (ed, wd) = detect_edge_scroll(&output.response, ui);
                edge_scroll_delta = ed;
                wheel_delta = wd;
            }
        });
        text_len_after_edit = viewport_buf.full_text.len();
        win_text_end = viewport_buf.window_end_byte.min(text_len_after_edit);
    } else {
        let mut viewport_buf = ReadonlyViewportTextBuffer {
            full_text: text.as_str(),
            window_start_byte: win_start_byte,
            window_end_byte: win_end_byte,
        };
        ui.horizontal_top(|ui| {
            ui.add_space(gutter_w + GUTTER_PAD + TEXT_PAD_LEFT);
            let output =
                code_text_edit(&mut viewport_buf, id, code_font.clone(), &mut layouter).show(ui);
            galley_y = output.galley_pos.y;
            out_galley = Some(output.galley.clone());
            out_galley_pos = output.galley_pos;
            if let Some(cr) = output.cursor_range {
                new_cursor_char = cr.primary.index + win_start_char;
                selection_secondary_global = cr.secondary.index + win_start_char;
            }
            if output.response.dragged() {
                let (ed, wd) = detect_edge_scroll(&output.response, ui);
                edge_scroll_delta = ed;
                wheel_delta = wd;
            }
        });
    }
    apply_scroll_delta(ui, edge_scroll_delta, wheel_delta);

    // 绘制选中同名高亮（跳过选区自身）
    // viewport 使用窗口本地偏移 + 窗口文本
    if let Some(g) = out_galley.as_ref() {
        let win_text = &text[win_text_start..win_text_end.min(text.len())];
        paint_word_highlight_overlay(
            ui,
            g,
            out_galley_pos,
            win_text,
            &local_word_ranges,
            new_cursor_char.saturating_sub(win_start_char),
            selection_secondary_global.saturating_sub(win_start_char),
            theme.word_highlight_bg,
        );
    }

    // 编辑后检查是否仍需视窗模式
    let is_still_viewport = !editable || !changed || text_len_after_edit > VIEWPORT_TEXT_LEN;

    // 选中同名字段高亮：从选区提取词，查找全文匹配
    let new_word = extract_highlight_word(text, new_cursor_char, selection_secondary_global);
    EditableLayoutCache::update_highlight_word(cache, text, new_word);

    if let Some(c) = cache.as_mut() {
        c.viewport_cursor_char = new_cursor_char;
        c.viewport_window_start = win_start_char;
        c.is_viewport = is_still_viewport;
        // 编辑后刷新全文统计缓存
        if changed {
            let new_len = text.len();
            c.full_text_len = new_len;
            c.full_text_chars = text.chars().count();
            c.full_text_lines = text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;
        }
    }

    // 计算窗口文本的行号映射
    // 起始行号 = 窗口前文本中的换行数 + 1
    let start_line = text.as_bytes()[..win_start_byte]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    let window_text = &text[win_start_byte..win_text_end.min(text.len())];
    let window_line_count = window_text
        .as_bytes()
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    let line_mapping: Vec<Option<u32>> = (0..window_line_count)
        .map(|i| Some((start_line + i) as u32))
        .collect();

    paint_line_numbers(
        ui,
        galley_y,
        window_line_count,
        &line_mapping,
        gutter_w,
        &code_font,
        theme,
    );
    paint_viewport_indicator(ui, win_start_char, win_end_char, total_chars, theme);
    changed
}

/// 视窗模式指示器（右下角显示当前窗口范围）
fn paint_viewport_indicator(
    ui: &egui::Ui,
    win_start: usize,
    win_end: usize,
    total: usize,
    theme: &CodeViewTheme,
) {
    let clip = ui.clip_rect();
    let painter = ui.painter();
    let label = format!("Viewport {win_start}..{win_end} / {total}");
    let font = egui::FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(label, font, theme.line_number_color);
    let size = galley.size();
    let pos = egui::pos2(clip.right() - size.x - 6.0, clip.bottom() - size.y - 4.0);
    // 半透明背景
    let bg_rect = egui::Rect::from_min_size(
        egui::pos2(pos.x - 4.0, pos.y - 2.0),
        egui::vec2(size.x + 8.0, size.y + 4.0),
    );
    painter.rect_filled(bg_rect, 3.0, theme.bg.gamma_multiply(0.85));
    painter.galley(pos, galley, theme.line_number_color);
}
