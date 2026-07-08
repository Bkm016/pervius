//! 搜索索引：反编译源码全文索引与搜索算法
//!
//! 索引在后台线程构建，搜索在后台线程执行，UI 线程零阻塞。
//!
//! @author sky

use pervius_java_bridge::decompiler;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;

/// 搜索结果上限
pub const MAX_MATCHES: usize = 100;

/// 搜索索引，包含所有已反编译类的源码
pub struct SearchIndex {
    /// 索引条目列表
    pub entries: Vec<IndexEntry>,
}

/// 单个类的索引条目
pub struct IndexEntry {
    /// JAR 内路径（如 "com/example/MyClass.class"）
    pub entry_path: String,
    /// 简短类名（如 "MyClass"）
    pub class_name: String,
    /// 包路径（如 "com.example"）
    pub package: String,
    /// 完整反编译源码
    pub source: String,
}

/// 索引构建请求（在 UI 线程提取，传给后台线程）
pub struct IndexBuildRequest {
    /// JAR 的 SHA-256 哈希
    pub hash: String,
    /// 所有 .class 条目路径
    pub class_paths: Vec<String>,
    /// 已修改条目的内存反编译缓存（预先 clone）
    pub modified_sources: HashMap<String, String>,
}

/// 增量搜索消息（后台线程 → UI 线程）
pub enum SearchMessage {
    /// 一个类的搜索结果
    Group(super::result::SearchResultGroup),
    /// 搜索完成
    Done(SearchDone),
}

/// 搜索完成时的汇总信息
pub struct SearchDone {
    /// 总匹配行数
    pub total_matches: usize,
    /// 是否因超过上限而截断
    pub truncated: bool,
    /// 有匹配结果的文件数
    pub files_matched: usize,
}

/// 从 entry_path 提取 class_name 和 package
fn split_class_path(path: &str) -> (String, String) {
    let base = path.strip_suffix(".class").unwrap_or(path);
    if let Some(idx) = base.rfind('/') {
        let class_name = &base[idx + 1..];
        let package = base[..idx].replace('/', ".");
        (class_name.to_string(), package)
    } else {
        (base.to_string(), String::new())
    }
}

/// 判断路径是否为内部类
///
/// 文件名含 `$` 且在第一个 `$` 处截断后对应的外部类文件存在于 JAR 中。
/// 如 `com/a/Outer$Inner.class` 的外部类为 `com/a/Outer.class`。
fn is_inner_class(path: &str, class_set: &HashSet<&str>) -> bool {
    let base = path.strip_suffix(".class").unwrap_or(path);
    let file_name = base.rsplit('/').next().unwrap_or(base);
    if !file_name.contains('$') {
        return false;
    }
    let prefix_len = base.len() - file_name.len();
    let dollar_pos = file_name.find('$').unwrap();
    let mut outer = String::with_capacity(prefix_len + dollar_pos + 6);
    outer.push_str(&base[..prefix_len + dollar_pos]);
    outer.push_str(".class");
    class_set.contains(outer.as_str())
}

/// 构建搜索索引（在后台线程执行）
///
/// 遍历所有 .class 条目，从磁盘缓存或内存缓存读取反编译源码。
/// `progress` 原子计数器每处理一个条目递增一次。
pub fn build_index(req: IndexBuildRequest, progress: &Arc<AtomicU32>) -> SearchIndex {
    let mut entries = Vec::new();
    // 收集所有 class 路径，用于判断内部类
    let class_set: HashSet<&str> = req.class_paths.iter().map(|s| s.as_str()).collect();
    for path in &req.class_paths {
        if !path.ends_with(".class") {
            continue;
        }
        // 跳过内部类：文件名含 '$' 且对应的外部类文件存在于 JAR 中
        if is_inner_class(path, &class_set) {
            progress.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        // 跳过内部类（文件名含 '$'），其源码已包含在外部类中
        if path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.contains('$'))
        {
            progress.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let source = if let Some(src) = req.modified_sources.get(path) {
            Some(src.clone())
        } else {
            decompiler::cached_source(&req.hash, path).map(|c| c.source)
        };
        if let Some(source) = source {
            let (class_name, package) = split_class_path(path);
            entries.push(IndexEntry {
                entry_path: path.clone(),
                class_name,
                package,
                source,
            });
        }
        progress.fetch_add(1, Ordering::Relaxed);
    }
    SearchIndex { entries }
}

/// 执行增量搜索（在后台线程执行）
///
/// 每搜完一个类且有匹配就立即通过 channel 发送 Group，
/// 全部完成后发送 Done 汇总。
pub fn search_streaming(
    index: &SearchIndex,
    query: &str,
    case_sensitive: bool,
    use_regex: bool,
    max_matches: usize,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<SearchMessage>,
) {
    use super::result::{LineMatch, SearchResultGroup};
    if query.is_empty() {
        let _ = tx.send(SearchMessage::Done(SearchDone {
            total_matches: 0,
            truncated: false,
            files_matched: 0,
        }));
        return;
    }
    let matcher = match build_matcher(query, case_sensitive, use_regex) {
        Some(m) => m,
        None => {
            let _ = tx.send(SearchMessage::Done(SearchDone {
                total_matches: 0,
                truncated: false,
                files_matched: 0,
            }));
            return;
        }
    };
    let mut total_matches = 0;
    let mut files_matched = 0;
    let mut truncated = false;
    for (entry_idx, entry) in index.entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let mut matches = Vec::new();
        for (line_idx, line) in entry.source.lines().enumerate() {
            let found: Vec<(usize, usize)> = matcher.find_all(line);
            if found.is_empty() {
                continue;
            }
            let trimmed = line.trim_start();
            let trim_offset = line.len() - trimmed.len();
            let highlights: Vec<(usize, usize)> = found
                .iter()
                .map(|&(s, e)| {
                    let hs = s.saturating_sub(trim_offset);
                    let he = e.saturating_sub(trim_offset).min(trimmed.len());
                    (hs, he)
                })
                .filter(|&(s, e)| s < e)
                .collect();
            if highlights.is_empty() {
                continue;
            }
            matches.push(LineMatch {
                line: line_idx,
                preview: trimmed.to_string(),
                highlights,
            });
            total_matches += 1;
            if total_matches >= max_matches {
                truncated = true;
                break;
            }
        }
        if !matches.is_empty() {
            files_matched += 1;
            let group = SearchResultGroup {
                class_name: entry.class_name.clone(),
                package: entry.package.clone(),
                entry_path: entry.entry_path.clone(),
                source_index: entry_idx,
                matches,
                expanded: true,
            };
            if tx.send(SearchMessage::Group(group)).is_err() {
                return;
            }
        }
        if truncated {
            break;
        }
    }
    let _ = tx.send(SearchMessage::Done(SearchDone {
        total_matches,
        truncated,
        files_matched,
    }));
}

/// 统一的匹配器：子串或正则
enum Matcher {
    Regex(Regex),
}

impl Matcher {
    /// 返回行内所有匹配的 (start, end) 字节区间
    fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        let Matcher::Regex(re) = self;
        re.find_iter(text).map(|m| (m.start(), m.end())).collect()
    }
}

/// 构建匹配器
///
/// 非正则模式下使用 `regex::escape` 转义查询文本。
/// 大小写不敏感时添加 `(?i)` 前缀。
fn build_matcher(query: &str, case_sensitive: bool, use_regex: bool) -> Option<Matcher> {
    let pattern = if use_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    let pattern = if case_sensitive {
        pattern
    } else {
        format!("(?i){pattern}")
    };
    Regex::new(&pattern).ok().map(Matcher::Regex)
}
