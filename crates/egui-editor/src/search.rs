//! 全文搜索算法（文本 + 字节）
//!
//! @author sky

/// 查找匹配项（字节偏移）
#[derive(Clone)]
pub struct FindMatch {
    /// 起始字节偏移
    pub start: usize,
    /// 结束字节偏移
    pub end: usize,
}

/// 在文本中搜索（支持大小写、全词匹配）
pub fn find_all(text: &str, query: &str, case_sensitive: bool, whole_word: bool) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        find_plain(text, query, whole_word)
    } else {
        find_case_insensitive(text, query, whole_word)
    }
}

/// 在字节数据中搜索 ASCII 文本
pub fn find_bytes(data: &[u8], query: &str, case_sensitive: bool) -> Vec<FindMatch> {
    if query.is_empty() || data.is_empty() {
        return Vec::new();
    }
    // 尝试解析为 hex 模式（如 "CA FE BA BE"）
    if let Some(hex_bytes) = parse_hex_query(query) {
        return find_bytes_raw(data, &hex_bytes);
    }
    let query_bytes = query.as_bytes();
    if data.len() < query_bytes.len() {
        return Vec::new();
    }
    let mut results = Vec::new();
    let end = data.len() - query_bytes.len() + 1;
    for i in 0..end {
        let matches = if case_sensitive {
            data[i..i + query_bytes.len()] == *query_bytes
        } else {
            data[i..i + query_bytes.len()]
                .iter()
                .zip(query_bytes)
                .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
        };
        if matches {
            results.push(FindMatch {
                start: i,
                end: i + query_bytes.len(),
            });
        }
    }
    results
}

/// 在字节数据中搜索原始字节序列（hex 模式，忽略大小写选项）
fn find_bytes_raw(data: &[u8], pattern: &[u8]) -> Vec<FindMatch> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return Vec::new();
    }
    let mut results = Vec::new();
    let end = data.len() - pattern.len() + 1;
    for i in 0..end {
        if data[i..i + pattern.len()] == *pattern {
            results.push(FindMatch {
                start: i,
                end: i + pattern.len(),
            });
        }
    }
    results
}

/// 尝试将查询解析为 hex 字节序列（如 "CAFEBABE" 或 "CA FE BA BE"）
pub fn parse_hex_query(query: &str) -> Option<Vec<u8>> {
    let cleaned: String = query.chars().filter(|c| !c.is_whitespace()).collect();
    tabookit::ensure!(!cleaned.is_empty() && cleaned.len() % 2 == 0);
    tabookit::ensure!(cleaned.chars().all(|c| c.is_ascii_hexdigit()));
    let bytes: Result<Vec<u8>, _> = (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16))
        .collect();
    bytes.ok()
}

fn find_plain(text: &str, query: &str, whole_word: bool) -> Vec<FindMatch> {
    let mut results = Vec::new();
    let mut start = 0;
    while let Some(pos) = text[start..].find(query) {
        let abs = start + pos;
        let end = abs + query.len();
        if !whole_word || is_word_boundary(text, abs, end) {
            results.push(FindMatch { start: abs, end });
        }
        // 按字符步进，避免落在多字节 UTF-8 字符中间
        let step = text[abs..].chars().next().map_or(1, |c| c.len_utf8());
        start = abs + step;
    }
    results
}

fn find_case_insensitive(text: &str, query: &str, whole_word: bool) -> Vec<FindMatch> {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();
    let mut results = Vec::new();
    let mut start = 0;
    while let Some(pos) = text_lower[start..].find(&query_lower) {
        let abs = start + pos;
        let end = abs + query_lower.len();
        if !whole_word || is_word_boundary(text, abs, end) {
            results.push(FindMatch { start: abs, end });
        }
        let step = text_lower[abs..].chars().next().map_or(1, |c| c.len_utf8());
        start = abs + step;
    }
    results
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = if start == 0 {
        true
    } else {
        text[..start]
            .chars()
            .next_back()
            .map_or(true, |c| !is_word_char(c))
    };
    let after = if end >= text.len() {
        true
    } else {
        text[end..]
            .chars()
            .next()
            .map_or(true, |c| !is_word_char(c))
    };
    before && after
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}
