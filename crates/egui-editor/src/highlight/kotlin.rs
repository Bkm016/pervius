//! Kotlin 着色规则（tree-sitter-kotlin）
//!
//! @author sky

use super::{Span, TokenKind};

/// 返回 Some 表示命中着色，None 表示继续深入子节点
pub fn classify(node: &tree_sitter::Node, source: &[u8]) -> Option<TokenKind> {
    let kind = node.kind();
    if is_inside_string_literal(node)
        && (is_literal_dollar_identifier(node, source) || !is_string_interpolation_node(node, kind))
    {
        return Some(TokenKind::String);
    }
    if let Some(token) = classify_jvm_identifier_suffix(node, source) {
        return Some(token);
    }
    match kind {
        // 关键字（叶节点）
        "val" | "var" | "fun" | "class" | "object" | "interface" | "enum" | "typealias" | "if"
        | "else" | "when" | "for" | "do" | "while" | "try" | "catch" | "throw" | "finally"
        | "import" | "package" | "is" | "!is" | "in" | "!in" | "as" | "as?" | "constructor"
        | "init" | "get" | "set" | "return" | "continue" | "break" | "return_at"
        | "continue_at" | "break_at" | "new" | "companion" | "by" | "where" => Some(TokenKind::Keyword),
        // 修饰符关键字
        "class_modifier"
        | "member_modifier"
        | "function_modifier"
        | "property_modifier"
        | "platform_modifier"
        | "variance_modifier"
        | "parameter_modifier"
        | "visibility_modifier"
        | "reification_modifier"
        | "inheritance_modifier" => Some(TokenKind::Keyword),
        // this / super
        "this_expression" | "super_expression" => Some(TokenKind::Keyword),
        // 字面量关键字
        "boolean_literal" | "null_literal" => Some(TokenKind::Keyword),
        "integer_literal" | "long_literal" | "hex_literal" | "bin_literal" | "unsigned_literal"
        | "real_literal" => Some(TokenKind::Number),
        // 字符串容器必须继续递归，否则会把 `${...}` 插值表达式整段吞成 String。
        "string_literal"
        | "line_string_literal"
        | "multi_line_string_literal"
        | "multiline_string_literal" => {
            if node.child_count() == 0 {
                Some(TokenKind::String)
            } else {
                None
            }
        }
        "string_content"
        | "line_str_text"
        | "multi_line_str_text"
        | "character_literal"
        | "character_escape_seq"
        | "escape_sequence" => Some(TokenKind::String),
        // 插值容器继续递归；真正的 `$` / `${` / `}` 分隔符低对比度显示。
        "line_str_ref"
        | "multi_line_str_ref"
        | "interpolated_identifier"
        | "interpolated_expression" => None,
        "interpolation_expression_start"
        | "interpolation_expression_end"
        | "interpolation_identifier_start" => Some(TokenKind::Muted),
        // 注释
        "line_comment" | "multiline_comment" | "shebang_line" => Some(TokenKind::Comment),
        // 注解：递归进子节点，让内部字符串正确着色
        "annotation" => None,
        "@" => Some(TokenKind::Annotation),
        // 类型（注解内的类型标识符着色为注解色）
        "type_identifier" => {
            if ancestors_contain(node, "annotation", 6) {
                Some(TokenKind::Annotation)
            } else {
                Some(TokenKind::Type)
            }
        }
        // 标识符
        "simple_identifier" => classify_identifier(node, source),
        // 标点
        "(" | ")" | "[" | "]" | "{" | "}" | "." | "," | ";" | ":" | "::" => Some(TokenKind::Muted),
        _ => None,
    }
}

fn classify_identifier(node: &tree_sitter::Node, source: &[u8]) -> Option<TokenKind> {
    if ancestors_contain(node, "import_header", 8) {
        return Some(TokenKind::Plain);
    }
    let parent = node.parent()?;
    match parent.kind() {
        // 类型声明名称
        "class_declaration" | "object_declaration" | "typealias_declaration"
            if is_first_identifier_child(&parent, node) =>
        {
            return Some(TokenKind::Type);
        }
        // enum 条目
        "enum_entry" if is_first_identifier_child(&parent, node) => {
            return Some(TokenKind::Constant);
        }
        // 函数声明的名称
        "function_declaration" if is_first_identifier_child(&parent, node) => {
            return Some(TokenKind::MethodDeclaration);
        }
        _ => {}
    }
    let text = node.utf8_text(source).unwrap_or("");
    if is_called_expression_leaf(node) {
        return Some(classify_called_identifier_text(text));
    }
    if parent.kind() == "navigation_suffix" {
        if is_upper_snake_case(text) {
            return Some(TokenKind::Constant);
        }
        if is_type_like_identifier(text) {
            return Some(TokenKind::Type);
        }
    }
    if parent.kind() == "navigation_expression"
        && is_first_identifier_child(&parent, node)
        && is_type_like_identifier(text)
    {
        return Some(TokenKind::Type);
    }
    if let Some(token) = classify_contextual_keyword_identifier(node, &parent, text, source) {
        return Some(token);
    }
    if is_upper_snake_case(text) {
        return Some(TokenKind::Constant);
    }
    None
}

fn is_upper_snake_case(text: &str) -> bool {
    text.len() >= 2 && text.chars().all(|c| c.is_ascii_uppercase() || c == '_')
}

fn is_type_like_identifier(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn classify_called_identifier_text(text: &str) -> TokenKind {
    if is_upper_snake_case(text) {
        TokenKind::Constant
    } else if is_type_like_identifier(text) {
        TokenKind::Type
    } else {
        TokenKind::MethodCall
    }
}

fn is_called_expression_leaf(node: &tree_sitter::Node) -> bool {
    let Some(call_expression) = first_ancestor_of_kind(node, "call_expression") else {
        return false;
    };
    call_expression.child(0).is_some_and(|callee| {
        callee.start_byte() <= node.start_byte() && callee.end_byte() == node.end_byte()
    })
}

fn first_ancestor_of_kind<'tree>(
    node: &tree_sitter::Node<'tree>,
    kind: &str,
) -> Option<tree_sitter::Node<'tree>> {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        if parent.kind() == kind {
            return Some(parent);
        }
        cur = parent.parent();
    }
    None
}

fn classify_contextual_keyword_identifier(
    node: &tree_sitter::Node,
    parent: &tree_sitter::Node,
    text: &str,
    source: &[u8],
) -> Option<TokenKind> {
    if !is_soft_keyword_identifier(text) {
        return None;
    }
    if kind_contains(parent.kind(), "modifier") || ancestors_kind_contains(node, "modifier", 4) {
        return Some(TokenKind::Keyword);
    }
    if is_annotation_use_site_target(node, parent, text, source) {
        return Some(TokenKind::Keyword);
    }
    match text {
        "where" if ancestors_kind_contains(node, "constraint", 6) => Some(TokenKind::Keyword),
        "by" if ancestors_kind_contains(node, "delegat", 6) => Some(TokenKind::Keyword),
        "field" if is_backing_field_identifier(node) => Some(TokenKind::Keyword),
        _ => None,
    }
}

fn is_soft_keyword_identifier(text: &str) -> bool {
    matches!(
        text,
        "abstract"
            | "actual"
            | "annotation"
            | "by"
            | "companion"
            | "const"
            | "crossinline"
            | "data"
            | "delegate"
            | "dynamic"
            | "expect"
            | "external"
            | "field"
            | "file"
            | "final"
            | "get"
            | "infix"
            | "inline"
            | "inner"
            | "internal"
            | "lateinit"
            | "noinline"
            | "open"
            | "operator"
            | "out"
            | "override"
            | "param"
            | "private"
            | "property"
            | "protected"
            | "public"
            | "receiver"
            | "sealed"
            | "set"
            | "setparam"
            | "suspend"
            | "tailrec"
            | "value"
            | "vararg"
            | "where"
    )
}

fn is_annotation_use_site_target(
    node: &tree_sitter::Node,
    parent: &tree_sitter::Node,
    text: &str,
    source: &[u8],
) -> bool {
    matches!(
        text,
        "field"
            | "file"
            | "property"
            | "get"
            | "set"
            | "receiver"
            | "param"
            | "setparam"
            | "delegate"
    ) && (kind_contains(parent.kind(), "use_site")
        || ancestors_kind_contains(node, "annotation", 6))
        && next_non_whitespace_byte_char(source, node.end_byte()) == Some(':')
}

fn is_backing_field_identifier(node: &tree_sitter::Node) -> bool {
    ancestors_kind_contains(node, "accessor", 8)
        || ancestors_kind_contains(node, "getter", 8)
        || ancestors_kind_contains(node, "setter", 8)
}

fn is_literal_dollar_identifier(node: &tree_sitter::Node, source: &[u8]) -> bool {
    (node.kind() == "interpolated_identifier"
        || node.kind() == "interpolation_identifier_start"
        || ancestors_contain(node, "interpolated_identifier", 4))
        && dollar_looks_like_jvm_string_identifier(node.start_byte(), source)
}

fn classify_jvm_identifier_suffix(node: &tree_sitter::Node, source: &[u8]) -> Option<TokenKind> {
    if !dollar_belongs_to_identifier(node.start_byte(), source) {
        return None;
    }
    if ancestors_contain(node, "function_declaration", 4) {
        return Some(TokenKind::MethodDeclaration);
    }
    let text = node.utf8_text(source).unwrap_or("");
    if is_called_expression_leaf(node) {
        return Some(classify_called_identifier_text(text));
    }
    if is_upper_snake_case(text) {
        return Some(TokenKind::Constant);
    }
    None
}

fn dollar_belongs_to_identifier(start: usize, source: &[u8]) -> bool {
    let Some(dollar) = dollar_index_for_node_start(start, source) else {
        return false;
    };
    dollar_has_identifier_neighbors(dollar, source)
}

fn dollar_looks_like_jvm_string_identifier(start: usize, source: &[u8]) -> bool {
    let Some(dollar) = dollar_index_for_node_start(start, source) else {
        return false;
    };
    if !dollar_has_identifier_neighbors(dollar, source) {
        return false;
    }
    source
        .get(previous_identifier_start(dollar, source))
        .is_some_and(|&b| b.is_ascii_uppercase())
        || source
            .get(dollar + 1)
            .is_some_and(|&b| b.is_ascii_uppercase())
}

fn dollar_index_for_node_start(start: usize, source: &[u8]) -> Option<usize> {
    if source.get(start) == Some(&b'$') {
        Some(start)
    } else {
        start
            .checked_sub(1)
            .filter(|&idx| source.get(idx) == Some(&b'$'))
    }
}

fn dollar_has_identifier_neighbors(dollar: usize, source: &[u8]) -> bool {
    dollar > 0
        && source
            .get(dollar - 1)
            .is_some_and(|&b| is_kotlin_identifier_byte(b))
        && source
            .get(dollar + 1)
            .is_some_and(|&b| is_kotlin_identifier_byte(b))
}

fn previous_identifier_start(mut index: usize, source: &[u8]) -> usize {
    while index > 0
        && source
            .get(index - 1)
            .is_some_and(|&b| is_kotlin_identifier_byte(b))
    {
        index -= 1;
    }
    index
}

fn is_kotlin_identifier_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// 补齐 Kotlin 字符串中 tree-sitter 未覆盖的片段（常见为引号或错误恢复产生的空白区）。
pub(super) fn patch_string_gaps(spans: &mut Vec<Span>, source: &str) {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let end = if source[i..].starts_with("\"\"\"") {
            find_raw_string_end(source, i + 3).unwrap_or(bytes.len())
        } else {
            find_line_string_end(bytes, i + 1)
        };
        fill_string_gaps(spans, source, i, end);
        i = end.max(i + 1);
    }
}

fn fill_string_gaps(spans: &mut Vec<Span>, source: &str, start: usize, end: usize) {
    let mut covered = spans
        .iter()
        .filter_map(|&(s, e, _)| (s < end && e > start).then_some((s.max(start), e.min(end))))
        .collect::<Vec<_>>();
    covered.sort_unstable();

    let mut cursor = start;
    for (s, e) in covered {
        if cursor < s {
            push_string_span(spans, source, cursor, s);
        }
        cursor = cursor.max(e);
    }
    push_string_span(spans, source, cursor, end);
}

fn push_string_span(spans: &mut Vec<Span>, source: &str, start: usize, end: usize) {
    if start < end && source.is_char_boundary(start) && source.is_char_boundary(end) {
        spans.push((start, end, TokenKind::String));
    }
}

fn find_raw_string_end(source: &str, from: usize) -> Option<usize> {
    source[from..].find("\"\"\"").map(|offset| from + offset + 3)
}

fn find_line_string_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = (i + 2).min(bytes.len()),
            b'"' => return i + 1,
            b'\n' | b'\r' => return i,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn is_inside_string_literal(node: &tree_sitter::Node) -> bool {
    ancestors_contain(node, "string_literal", 16)
        || ancestors_contain(node, "line_string_literal", 16)
        || ancestors_contain(node, "multi_line_string_literal", 16)
        || ancestors_contain(node, "multiline_string_literal", 16)
}

fn is_string_interpolation_node(node: &tree_sitter::Node, kind: &str) -> bool {
    matches!(
        kind,
        "interpolated_expression"
            | "interpolated_identifier"
            | "interpolation_expression_start"
            | "interpolation_expression_end"
            | "interpolation_identifier_start"
            | "line_str_ref"
            | "multi_line_str_ref"
    ) || ancestors_contain(node, "interpolated_expression", 16)
        || ancestors_contain(node, "interpolated_identifier", 16)
}

fn kind_contains(kind: &str, needle: &str) -> bool {
    kind.contains(needle)
}

fn ancestors_kind_contains(node: &tree_sitter::Node, needle: &str, max_depth: usize) -> bool {
    let mut cur = node.parent();
    for _ in 0..max_depth {
        match cur {
            Some(n) if kind_contains(n.kind(), needle) => return true,
            Some(n) => cur = n.parent(),
            None => return false,
        }
    }
    false
}

fn next_non_whitespace_byte_char(source: &[u8], from: usize) -> Option<char> {
    std::str::from_utf8(source.get(from..)?)
        .ok()?
        .chars()
        .find(|c| !c.is_whitespace())
}

fn is_first_identifier_child(parent: &tree_sitter::Node, node: &tree_sitter::Node) -> bool {
    for i in 0..parent.child_count() {
        if let Some(child) = parent.child(i) {
            if child.kind() == "simple_identifier" {
                return child.id() == node.id();
            }
        }
    }
    false
}

/// 在 max_depth 层祖先内查找指定 kind
fn ancestors_contain(node: &tree_sitter::Node, kind: &str, max_depth: usize) -> bool {
    let mut cur = node.parent();
    for _ in 0..max_depth {
        match cur {
            Some(n) if n.kind() == kind => return true,
            Some(n) => cur = n.parent(),
            None => return false,
        }
    }
    false
}
