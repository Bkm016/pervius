//! YAML 着色规则
//!
//! @author sky

use super::TokenKind;

pub fn classify(node: &tree_sitter::Node, _source: &[u8]) -> Option<TokenKind> {
    match node.kind() {
        "double_quote_scalar" | "single_quote_scalar" | "string_scalar" | "block_scalar" => {
            Some(TokenKind::String)
        }
        "integer_scalar" | "float_scalar" => Some(TokenKind::Number),
        "boolean_scalar" | "null_scalar" => Some(TokenKind::Keyword),
        "comment" => Some(TokenKind::Comment),
        "tag" | "anchor" | "alias" => Some(TokenKind::Annotation),
        _ => {
            // key 判断：block_mapping_pair / flow_pair 的 key 字段
            let parent = node.parent()?;
            if (parent.kind() == "block_mapping_pair" || parent.kind() == "flow_pair")
                && parent
                    .child_by_field_name("key")
                    .is_some_and(|n| n.id() == node.id())
            {
                return Some(TokenKind::Constant);
            }
            None
        }
    }
}
