//! Ctrl+Click 导航逻辑
//!
//! @author sky

use crate::highlight::{Span, TokenKind};
use crate::theme::CodeViewTheme;
use eframe::egui;
use std::collections::HashSet;

/// Ctrl+Click 导航请求（由 code_view 产生，调用方消费）
pub struct NavigationHit {
    /// 点击的 token 文本（如 "MyClass"、"getValue"）
    pub token: String,
    /// token 的语法类型
    pub kind: TokenKind,
    /// 调用者文本（仅 MethodCall/Constant，如 "obj.method()" 中的 "obj"）
    pub receiver: Option<String>,
    /// 是否为声明处的 token（MethodDeclaration 等），声明处触发 Find Usages
    pub is_declaration: bool,
}

/// 可导航的 token 类型
fn is_navigable(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Type
            | TokenKind::MethodCall
            | TokenKind::Constant
            | TokenKind::MethodDeclaration
    )
}

/// 在已排序的 span 列表中查找包含指定字节偏移的 span
fn find_span_at(spans: &[Span], byte_offset: usize) -> Option<&Span> {
    // 二分查找：找到最后一个 start <= byte_offset 的 span
    let idx = spans.partition_point(|s| s.0 <= byte_offset);
    if idx == 0 {
        return None;
    }
    let span = &spans[idx - 1];
    if byte_offset < span.1 {
        Some(span)
    } else {
        None
    }
}

/// 从 TextEdit 光标位置提取字节偏移
fn cursor_byte_offset(text: &str, ccursor: &egui::text::CCursor) -> usize {
    super::byte_offset_at_char(text, ccursor.index)
}

/// 检测 Ctrl+Click 和 Ctrl+Hover，处理导航提示与点击
pub(crate) fn detect_navigation(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &str,
    spans: &[Span],
    theme: &CodeViewTheme,
    known_classes: Option<&HashSet<String>>,
) -> Option<NavigationHit> {
    let ctrl = ui.input(|i| i.modifiers.command_only());
    if !ctrl {
        return None;
    }
    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    let Some(pos) = hover_pos else {
        return None;
    };
    // 检查鼠标是否在 galley 区域内
    let galley_rect = egui::Rect::from_min_size(output.galley_pos, output.galley.size());
    if !galley_rect.contains(pos) {
        return None;
    }
    let local = pos - output.galley_pos;
    let cursor = output.galley.cursor_from_pos(local);
    let byte_offset = cursor_byte_offset(text, &cursor);
    let span = find_span_at(spans, byte_offset)?;
    if !is_navigable(span.2) {
        return None;
    }
    let token = &text[span.0..span.1];
    // 根据 known_classes 过滤：只在确信能跳转时才显示 hover
    if let Some(names) = known_classes {
        let resolvable = match span.2 {
            TokenKind::Type => names.contains(token),
            TokenKind::MethodCall | TokenKind::Constant => {
                // 必须有大写开头的 receiver（类名.方法/字段），且该类在 JAR 内
                match extract_receiver(text, span) {
                    Some(ref r) if r.starts_with(|c: char| c.is_uppercase()) => {
                        names.contains(r.as_str())
                    }
                    _ => false,
                }
            }
            // 方法声明：始终可导航（Find Usages）
            TokenKind::MethodDeclaration => true,
            _ => false,
        };
        if !resolvable {
            return None;
        }
    }
    // Hover 反馈：手型光标 + 下划线
    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    paint_token_underline(ui, output, text, span, theme);
    // Ctrl+Click → 产生导航请求
    let clicked = output.response.clicked();
    if clicked {
        let is_declaration = matches!(span.2, TokenKind::MethodDeclaration);
        let receiver = extract_receiver(text, span);
        Some(NavigationHit {
            token: token.to_string(),
            kind: span.2,
            receiver,
            is_declaration,
        })
    } else {
        None
    }
}

/// 绘制 token 下划线
fn paint_token_underline(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &str,
    span: &Span,
    theme: &CodeViewTheme,
) {
    let start_char = text[..span.0].chars().count();
    let end_char = start_char + text[span.0..span.1].chars().count();
    let start_cursor = egui::text::CCursor::new(start_char);
    let end_cursor = egui::text::CCursor::new(end_char);
    let start_rect = output.galley.pos_from_cursor(start_cursor);
    let end_rect = output.galley.pos_from_cursor(end_cursor);
    // 同一行时绘制下划线
    if (start_rect.top() - end_rect.top()).abs() < 1.0 {
        let y = output.galley_pos.y + start_rect.bottom() - 1.0;
        let x_start = output.galley_pos.x + start_rect.left();
        let x_end = output.galley_pos.x + end_rect.left();
        let color = span.2.color(&theme.syntax);
        ui.painter().line_segment(
            [egui::pos2(x_start, y), egui::pos2(x_end, y)],
            egui::Stroke::new(1.0, color),
        );
    }
}

/// 从 token 前文提取调用者（如 "obj.method()" 中的 "obj"）
fn extract_receiver(text: &str, span: &Span) -> Option<String> {
    if !matches!(span.2, TokenKind::MethodCall | TokenKind::Constant) {
        return None;
    }
    // 向前检查是否有 "."
    let before = &text[..span.0];
    let trimmed = before.trim_end();
    let trimmed = trimmed.strip_suffix('`').unwrap_or(trimmed).trim_end();
    if !trimmed.ends_with('.') {
        return None;
    }
    let before_dot = trimmed[..trimmed.len() - 1].trim_end();
    // 提取 receiver 标识符（往前取连续的字母数字下划线）
    let receiver: String = before_dot
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if receiver.is_empty() {
        None
    } else {
        Some(receiver)
    }
}
