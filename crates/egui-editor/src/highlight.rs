//! 语法高亮引擎
//!
//! 通用 tree-sitter 遍历框架 + 各语言着色规则分发。
//! 输出 egui LayoutJob 供 TextEdit 直接使用。
//! 仅覆盖 JAR 内可能出现的文件类型。
//!
//! @author sky

#[macro_use]
mod macros;
mod bytecode;
mod html;
mod java;
mod json;
mod kotlin;
mod properties;
mod sql;
mod xml;
mod yaml;

use crate::theme::{CodeViewTheme, SyntaxTheme};
use eframe::egui;
use eframe::egui::text::LayoutJob;

/// 语法 token 类型
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    /// 普通文本
    Plain,
    /// 语言关键字（access modifier、声明关键字等）
    Keyword,
    /// JVM 操作码（INVOKEVIRTUAL、ALOAD 等字节码指令）
    Opcode,
    /// 字符串字面量
    String,
    /// 类型名
    Type,
    /// 数字字面量
    Number,
    /// 注释
    Comment,
    /// 注解
    Annotation,
    /// 低对比度文本（标点、分隔符）
    Muted,
    /// 常量（enum 值、static final 字段）
    Constant,
    /// 方法调用
    MethodCall,
    /// 方法声明
    MethodDeclaration,
}

impl TokenKind {
    /// 映射到 egui 颜色
    pub fn color(self, theme: &SyntaxTheme) -> egui::Color32 {
        match self {
            Self::Plain => theme.text,
            Self::Keyword => theme.keyword,
            Self::Opcode => theme.opcode,
            Self::String => theme.string,
            Self::Type => theme.type_name,
            Self::Number => theme.number,
            Self::Comment => theme.comment,
            Self::Annotation => theme.annotation,
            Self::Muted => theme.muted,
            Self::Constant => theme.constant,
            Self::MethodCall => theme.method_call,
            Self::MethodDeclaration => theme.method_declaration,
        }
    }
}

define_languages! {
    ts {
        Java(java) = tree_sitter_java::LANGUAGE => ["java"],
        /// Kotlin（tree-sitter-kotlin）
        Kotlin(kotlin) = tree_sitter_kotlin_sg::LANGUAGE => ["kt", "kts"],
        Xml(xml) = tree_sitter_xml::LANGUAGE_XML => ["xml", "fxml", "pom"],
        Yaml(yaml) = tree_sitter_yaml::LANGUAGE => ["yml", "yaml"],
        Json(json) = tree_sitter_json::LANGUAGE => ["json", "jsonc", "mcmeta"],
        Html(html) = tree_sitter_html::LANGUAGE => ["html", "htm"],
        Sql(sql) = tree_sitter_sequel::LANGUAGE => ["sql"],
    }
    custom {
        Properties(properties) => ["properties", "cfg", "ini", "toml"],
        /// JVM 字节码（自定义 tokenizer，非 tree-sitter）
        Bytecode(bytecode),
    }
}

impl Language {
    /// 从文件名推断
    pub fn from_filename(name: &str) -> Self {
        if let Some(ext) = name.rsplit('.').next() {
            let lang = Self::from_extension(ext);
            if lang != Self::Plain {
                return lang;
            }
        }
        let upper = name.to_uppercase();
        if upper == "MANIFEST.MF" || upper.ends_with(".MF") {
            return Self::Properties;
        }
        Self::Plain
    }
}

/// 一个 span = (byte_start, byte_end, kind)
pub type Span = (usize, usize, TokenKind);

/// 预计算源码的语法 span（供虚拟滚动逐行渲染使用）
pub fn compute_spans(source: &str, lang: Language) -> Vec<Span> {
    collect_spans(source, lang)
}

/// 计算每行在源码中的字节偏移 + 最大行长（字节数）
pub fn compute_line_starts(source: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut offset = 0usize;
    for line in source.split('\n') {
        starts.push(offset);
        offset += line.len() + 1;
    }
    if starts.is_empty() {
        starts.push(0);
    }
    starts
}

fn collect_treesitter_spans(source: &str, lang: Language) -> Vec<Span> {
    collect_treesitter_spans_checked(source, lang).0
}

fn collect_treesitter_spans_checked(source: &str, lang: Language) -> (Vec<Span>, bool) {
    let (ts_lang, color_fn) = match resolve_treesitter(lang) {
        Some(pair) => pair,
        None => return (vec![(0, source.len(), TokenKind::Plain)], false),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return (vec![(0, source.len(), TokenKind::Plain)], true);
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return (vec![(0, source.len(), TokenKind::Plain)], true),
    };
    let has_errors = tree.root_node().has_error();
    let mut spans = Vec::new();
    collect_node_spans(
        &mut tree.root_node().walk(),
        &mut spans,
        color_fn,
        source.as_bytes(),
    );
    if lang == Language::Kotlin {
        kotlin::patch_string_gaps(&mut spans, source);
    }
    patch_dollar_identifier_spans(&mut spans, source);
    spans.sort_by_key(|&(start, _, _)| start);
    normalize_spans(&mut spans);
    (spans, has_errors)
}

fn normalize_spans(spans: &mut Vec<Span>) {
    if spans.is_empty() {
        return;
    }
    spans.sort_by_key(|&(start, end, kind)| (start, token_priority(kind), end - start));
    let mut write = 0usize;
    let mut covered_until = 0usize;
    for read in 0..spans.len() {
        let (mut start, end, kind) = spans[read];
        if start < covered_until {
            start = covered_until;
        }
        if start >= end {
            continue;
        }
        spans[write] = (start, end, kind);
        write += 1;
        covered_until = end;
    }
    spans.truncate(write);
    spans.sort_by_key(|&(start, _, _)| start);
}

fn patch_dollar_identifier_spans(spans: &mut Vec<Span>, source: &str) {
    if spans.len() < 2 {
        return;
    }
    let mut dollar_spans = Vec::new();
    let mut i = 0usize;
    while i + 1 < spans.len() {
        if !has_dollar_identifier_boundary(spans[i], spans[i + 1], source) {
            i += 1;
            continue;
        }
        let chain_start = i;
        let mut chain_end = i + 1;
        while chain_end + 1 < spans.len()
            && has_dollar_identifier_boundary(spans[chain_end], spans[chain_end + 1], source)
        {
            chain_end += 1;
        }
        let mut merged_kind = spans[chain_start..=chain_end]
            .iter()
            .fold(TokenKind::Plain, |kind, &(_, _, next_kind)| {
                merge_identifier_kind(kind, next_kind)
            });
        let last_end = spans[chain_end].1;
        if merged_kind != TokenKind::MethodDeclaration
            && next_non_whitespace_char(source, last_end) == Some('(')
        {
            merged_kind = TokenKind::MethodCall;
        }
        for idx in chain_start..=chain_end {
            spans[idx].2 = merged_kind;
        }
        for idx in chain_start..chain_end {
            let left_end = spans[idx].1;
            let right_start = spans[idx + 1].0;
            dollar_spans.push((left_end, right_start, merged_kind));
        }
        i = chain_end + 1;
    }
    spans.extend(dollar_spans);
}

fn has_dollar_identifier_boundary(left: Span, right: Span, source: &str) -> bool {
    let (_, left_end, left_kind) = left;
    let (right_start, _, right_kind) = right;
    if left_end >= right_start || right_start != left_end + 1 {
        return false;
    }
    if source.as_bytes().get(left_end) != Some(&b'$') {
        return false;
    }
    if !is_identifier_like_kind(left_kind) || !is_identifier_like_kind(right_kind) {
        return false;
    }
    if source[..left_end]
        .chars()
        .next_back()
        .map_or(true, |c| !is_identifier_char(c))
        || source[right_start..]
            .chars()
            .next()
            .map_or(true, |c| !is_identifier_char(c))
    {
        return false;
    }
    true
}

fn is_identifier_like_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plain
            | TokenKind::Type
            | TokenKind::Constant
            | TokenKind::MethodCall
            | TokenKind::MethodDeclaration
    )
}

fn merge_identifier_kind(left: TokenKind, right: TokenKind) -> TokenKind {
    use TokenKind::*;
    match (left, right) {
        (MethodDeclaration, _) | (_, MethodDeclaration) => MethodDeclaration,
        (MethodCall, _) | (_, MethodCall) => MethodCall,
        (Type, _) | (_, Type) => Type,
        (Constant, _) | (_, Constant) => Constant,
        _ => Plain,
    }
}

fn next_non_whitespace_char(source: &str, from: usize) -> Option<char> {
    source[from..].chars().find(|c| !c.is_whitespace())
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn token_priority(kind: TokenKind) -> u8 {
    match kind {
        TokenKind::String | TokenKind::Comment => 0,
        TokenKind::Plain | TokenKind::Muted => 2,
        _ => 1,
    }
}

/// 各语言着色函数签名：返回 Some 表示命中，None 表示继续深入子节点
type ColorFn = fn(&tree_sitter::Node, &[u8]) -> Option<TokenKind>;

/// 一次深度优先遍历，收集所有叶子 span（字节偏移）
fn collect_node_spans(
    cursor: &mut tree_sitter::TreeCursor,
    spans: &mut Vec<Span>,
    color_fn: ColorFn,
    source: &[u8],
) {
    loop {
        let node = cursor.node();
        let kind = color_fn(&node, source);
        if kind.is_some() || node.child_count() == 0 {
            spans.push((
                node.start_byte(),
                node.end_byte(),
                kind.unwrap_or(TokenKind::Plain),
            ));
            if !advance_cursor(cursor) {
                return;
            }
            continue;
        }
        if cursor.goto_first_child() {
            continue;
        }
        if !advance_cursor(cursor) {
            return;
        }
    }
}

/// 移动到下一个兄弟，若无兄弟则回溯父节点。返回 false 表示遍历结束
fn advance_cursor(cursor: &mut tree_sitter::TreeCursor) -> bool {
    if cursor.goto_next_sibling() {
        return true;
    }
    loop {
        if !cursor.goto_parent() {
            return false;
        }
        if cursor.goto_next_sibling() {
            return true;
        }
    }
}

/// 将字节偏移对齐到最近的 char boundary（向前对齐），超出范围则截断到 source.len()
fn snap_char(source: &str, pos: usize) -> usize {
    let p = pos.min(source.len());
    if source.is_char_boundary(p) {
        return p;
    }
    // 向前找到最近的 char boundary
    let mut i = p;
    while i > 0 && !source.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// 从排序好的 span 列表构建 LayoutJob，自动填充 gap
///
/// span 偏移可能来自旧版本文本（编辑延迟刷新），会自动对齐到 char boundary。
pub fn build_layout_job(
    source: &str,
    spans: &[Span],
    font: &egui::FontId,
    syntax: &SyntaxTheme,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut pos = 0usize;
    for &(start, end, kind) in spans {
        let start = snap_char(source, start);
        let end = snap_char(source, end);
        // gap 填充（未着色区域用 Plain）
        if start > pos {
            append_section(
                &mut job,
                &source[pos..start],
                TokenKind::Plain,
                font,
                syntax,
            );
        }
        let actual_start = start.max(pos);
        if end > actual_start {
            append_section(&mut job, &source[actual_start..end], kind, font, syntax);
            pos = end;
        }
    }
    // 尾部剩余
    if pos < source.len() {
        append_section(&mut job, &source[pos..], TokenKind::Plain, font, syntax);
    }
    job
}

/// 构建全文 LayoutJob：语法高亮 + 搜索匹配背景 + 选中同名高亮
///
/// `match_ranges` 为搜索匹配的字节偏移范围，`current_match` 为当前选中的匹配索引。
/// `word_ranges` 为选中同名字段的字节偏移范围（优先级低于搜索匹配）。
/// 无匹配时退化为 `build_layout_job`。
pub fn build_layout_job_with_matches(
    source: &str,
    spans: &[Span],
    match_ranges: &[(usize, usize)],
    current_match: Option<usize>,
    word_ranges: &[(usize, usize)],
    theme: &CodeViewTheme,
) -> LayoutJob {
    let font = egui::FontId::monospace(theme.code_font_size);
    if match_ranges.is_empty() && word_ranges.is_empty() {
        return build_layout_job(source, spans, &font, &theme.syntax);
    }
    let mut breaks = std::collections::BTreeSet::new();
    breaks.insert(0);
    breaks.insert(source.len());
    for &(s, e, _) in spans {
        breaks.insert(snap_char(source, s));
        breaks.insert(snap_char(source, e));
    }
    for &(s, e) in match_ranges {
        breaks.insert(snap_char(source, s));
        breaks.insert(snap_char(source, e));
    }
    for &(s, e) in word_ranges {
        breaks.insert(snap_char(source, s));
        breaks.insert(snap_char(source, e));
    }
    let breaks: Vec<usize> = breaks.into_iter().collect();
    let mut job = LayoutJob::default();
    for w in breaks.windows(2) {
        let (start, end) = (w[0], w[1]);
        if start >= end || start >= source.len() {
            continue;
        }
        let end = end.min(source.len());
        let color = spans
            .iter()
            .find(|&&(s, e, _)| s <= start && end <= e)
            .map(|&(_, _, k)| k.color(&theme.syntax))
            .unwrap_or(TokenKind::Plain.color(&theme.syntax));
        let match_idx = match_ranges
            .iter()
            .position(|&(s, e)| start >= s && end <= e);
        let is_current = matches!((match_idx, current_match), (Some(mi), Some(cm)) if mi == cm);
        let mut format = egui::TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        };
        // 搜索匹配优先级高于选中同名高亮
        if is_current {
            format.background = theme.search_current_bg;
        } else if match_idx.is_some() {
            format.background = theme.search_bg;
        }
        job.append(&source[start..end], 0.0, format);
    }
    job
}

fn append_section(
    job: &mut LayoutJob,
    text: &str,
    kind: TokenKind,
    font: &egui::FontId,
    syntax: &SyntaxTheme,
) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color: kind.color(syntax),
            ..Default::default()
        },
    );
}

/// 为搜索预览面板生成逐行 LayoutJob（含语法高亮 + 匹配区间背景标记）
///
/// `match_line` 对应的行会在 `match_ranges`（行内字节偏移）区间叠加 `match_bg` 背景色，
/// 使匹配文本在语法着色之上仍能辨识。
pub fn highlight_per_line(
    lines: &[String],
    lang: Language,
    font: egui::FontId,
    match_line: usize,
    match_ranges: &[(usize, usize)],
    match_bg: egui::Color32,
    syntax: &SyntaxTheme,
) -> Vec<LayoutJob> {
    let source = lines.join("\n");
    let spans = collect_spans(&source, lang);
    // 每行在 source 中的起始字节偏移
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut off = 0usize;
    for line in lines {
        line_starts.push(off);
        off += line.len() + 1;
    }
    let mut jobs = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let ls = line_starts[i];
        let le = ls + line.len();
        // 裁剪 + 转行内相对偏移
        let line_spans: Vec<Span> = spans
            .iter()
            .filter(|&&(s, e, _)| s < le && e > ls)
            .map(|&(s, e, k)| (s.max(ls) - ls, e.min(le) - ls, k))
            .collect();
        let mr = if i == match_line { match_ranges } else { &[] };
        jobs.push(build_line_layout(
            line,
            &line_spans,
            &font,
            mr,
            match_bg,
            syntax,
        ));
    }
    jobs
}

/// 合并语法 span 与匹配区间，输出单行 LayoutJob
fn build_line_layout(
    line: &str,
    spans: &[Span],
    font: &egui::FontId,
    match_ranges: &[(usize, usize)],
    match_bg: egui::Color32,
    syntax: &SyntaxTheme,
) -> LayoutJob {
    let mut breaks = std::collections::BTreeSet::new();
    breaks.insert(0);
    breaks.insert(line.len());
    for &(s, e, _) in spans {
        breaks.insert(s.min(line.len()));
        breaks.insert(e.min(line.len()));
    }
    for &(s, e) in match_ranges {
        breaks.insert(s.min(line.len()));
        breaks.insert(e.min(line.len()));
    }
    let breaks: Vec<usize> = breaks.into_iter().collect();
    let mut job = LayoutJob::default();
    for w in breaks.windows(2) {
        let (start, end) = (w[0], w[1]);
        if start >= end || start >= line.len() {
            continue;
        }
        let end = end.min(line.len());
        let color = spans
            .iter()
            .find(|&&(s, e, _)| s <= start && end <= e)
            .map(|&(_, _, k)| k.color(syntax))
            .unwrap_or(TokenKind::Plain.color(syntax));
        let in_match = match_ranges.iter().any(|&(s, e)| start >= s && end <= e);
        let mut format = egui::TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        };
        if in_match {
            format.background = match_bg;
        }
        job.append(&line[start..end], 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(source: &str) -> Vec<Span> {
        compute_spans(source, Language::Kotlin)
    }

    fn nth_range(source: &str, needle: &str, occurrence: usize) -> (usize, usize) {
        let mut from = 0usize;
        for index in 0..=occurrence {
            let offset = source[from..]
                .find(needle)
                .unwrap_or_else(|| panic!("missing occurrence {index} of {needle:?}"));
            let start = from + offset;
            let end = start + needle.len();
            if index == occurrence {
                return (start, end);
            }
            from = end;
        }
        unreachable!()
    }

    fn kind_at(source: &str, spans: &[Span], needle: &str, occurrence: usize) -> TokenKind {
        let (start, end) = nth_range(source, needle, occurrence);
        spans
            .iter()
            .find(|&&(s, e, _)| s <= start && end <= e)
            .map(|&(_, _, kind)| kind)
            .unwrap_or(TokenKind::Plain)
    }

    #[test]
    fn kotlin_highlights_type_identifiers() {
        let source = "class Foo(val value: Bar) { fun make(): Baz = Baz() }";
        let spans = spans(source);

        assert_eq!(kind_at(source, &spans, "Foo", 0), TokenKind::Type);
        assert_eq!(kind_at(source, &spans, "Bar", 0), TokenKind::Type);
        assert_eq!(kind_at(source, &spans, "Baz", 0), TokenKind::Type);
        assert_eq!(kind_at(source, &spans, "Baz", 1), TokenKind::Type);
    }

    #[test]
    fn kotlin_soft_keywords_are_contextual() {
        let source = "fun demo() { val value = 1; val field = value; val property = field }";
        let spans = spans(source);

        assert_ne!(kind_at(source, &spans, "value", 0), TokenKind::Keyword);
        assert_ne!(kind_at(source, &spans, "field", 0), TokenKind::Keyword);
        assert_ne!(kind_at(source, &spans, "property", 0), TokenKind::Keyword);
    }

    #[test]
    fn kotlin_string_interpolation_keeps_expression_highlighting() {
        let source = "fun demo(name: String, value: Int) { val s = \"hello$name ${format(value)}\" }";
        let spans = spans(source);

        assert_ne!(kind_at(source, &spans, "name", 1), TokenKind::String);
        assert_eq!(kind_at(source, &spans, "format", 0), TokenKind::MethodCall);
        assert_ne!(kind_at(source, &spans, "value", 1), TokenKind::String);
    }

    #[test]
    fn kotlin_jvm_dollar_names_inside_strings_remain_strings() {
        let source = "fun demo() { val s = \"pkg.Outer$Inner\" }";
        let spans = spans(source);

        assert_eq!(kind_at(source, &spans, "Outer", 0), TokenKind::String);
        assert_eq!(kind_at(source, &spans, "Inner", 0), TokenKind::String);
    }
}
