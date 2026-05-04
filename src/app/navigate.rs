//! Ctrl+Click 导航跳转：类、方法、字段
//!
//! 从反编译源码中的 token 解析出目标 class entry_path 和行号，
//! 支持 import 解析、同包推断、通配符匹配。
//! 声明处（MethodDeclaration）触发 Find Usages。
//!
//! @author sky

use std::collections::{HashMap, HashSet};

#[allow(dead_code)]
/// 导航跳转的完整请求（App 层消费）
pub struct NavigateRequest {
    /// 目标 entry_path（如 "com/example/MyClass.class"）
    pub entry_path: String,
    /// 目标行号（0-based），None 表示仅打开 tab
    pub line: Option<usize>,
}

/// 类名解析器（JAR 加载时构建）
pub struct ClassResolver {
    /// 简短类名 → entry_path 列表（同名类可能分布在不同包）
    simple_to_paths: HashMap<String, Vec<String>>,
    /// 完全限定名 → entry_path（"com.example.MyClass" → "com/example/MyClass.class"）
    fqn_to_path: HashMap<String, String>,
    /// 所有简短类名集合（用于 hover 快速判断）
    known_names: HashSet<String>,
}

impl ClassResolver {
    /// 从 JAR 内所有路径构建索引
    pub fn build(paths: &[&str]) -> Self {
        let mut simple_to_paths: HashMap<String, Vec<String>> = HashMap::new();
        let mut fqn_to_path = HashMap::new();
        for &path in paths {
            if !path.ends_with(".class") {
                continue;
            }
            let stem = &path[..path.len() - 6]; // 去 ".class"
            let fqn = stem.replace('/', ".");
            let simple = fqn.rsplit('.').next().unwrap_or(&fqn).to_string();
            simple_to_paths
                .entry(simple)
                .or_default()
                .push(path.to_string());
            fqn_to_path.insert(fqn, path.to_string());
        }
        let known_names: HashSet<String> = simple_to_paths.keys().cloned().collect();
        Self {
            simple_to_paths,
            fqn_to_path,
            known_names,
        }
    }

    /// 已知简短类名集合（hover 过滤用）
    pub fn known_names(&self) -> &HashSet<String> {
        &self.known_names
    }

    /// 解析类型名 → entry_path
    ///
    /// `type_name` 可以是简短名（"MyClass"）或限定名（"com.example.MyClass"）。
    /// `source` 是当前文件的反编译源码，用于提取 import 语句。
    /// `current_entry` 是当前文件的 entry_path，用于推断同包。
    pub fn resolve(&self, type_name: &str, source: &str, current_entry: &str) -> Option<String> {
        // 限定名直接查
        if type_name.contains('.') {
            return self.resolve_dotted(type_name);
        }
        // 基本类型和常见 JDK 类型不跳转
        if is_primitive_or_boxed(type_name) {
            return None;
        }
        let (imports, wildcards) = parse_imports(source);
        let current_pkg = package_from_entry(current_entry);
        // 1. import 精确匹配
        if let Some(fqn) = imports.get(type_name) {
            if let Some(path) = self.fqn_to_path.get(fqn) {
                return Some(path.clone());
            }
            // import 了但 JAR 里没有（JDK 类等），放弃
            return None;
        }
        // 2. 同包
        if let Some(pkg) = &current_pkg {
            let fqn = format!("{pkg}.{type_name}");
            if let Some(path) = self.fqn_to_path.get(&fqn) {
                return Some(path.clone());
            }
        }
        // 3. 通配符 import
        for wildcard_pkg in &wildcards {
            let fqn = format!("{wildcard_pkg}.{type_name}");
            if let Some(path) = self.fqn_to_path.get(&fqn) {
                return Some(path.clone());
            }
        }
        // 4. java.lang 隐式 import
        {
            let fqn = format!("java.lang.{type_name}");
            if let Some(path) = self.fqn_to_path.get(&fqn) {
                return Some(path.clone());
            }
        }
        // 5. 唯一匹配降级
        if let Some(paths) = self.simple_to_paths.get(type_name) {
            if paths.len() == 1 {
                return Some(paths[0].clone());
            }
        }
        None
    }

    /// 解析含 "." 的类型名（可能是内部类 Outer.Inner 或完全限定名）
    fn resolve_dotted(&self, name: &str) -> Option<String> {
        // 先尝试当作完全限定名
        if let Some(path) = self.fqn_to_path.get(name) {
            return Some(path.clone());
        }
        // 尝试内部类：把最后一个 "." 替换为 "$"
        let dollar = name.replacen('.', "$", name.matches('.').count());
        if let Some(path) = self.fqn_to_path.get(&dollar) {
            return Some(path.clone());
        }
        // 逐级尝试 "$" 替换（嵌套内部类）
        for (i, _) in name.rmatch_indices('.') {
            let candidate = format!("{}.{}", &name[..i], name[i + 1..].replace('.', "$"));
            if let Some(path) = self.fqn_to_path.get(&candidate) {
                return Some(path.clone());
            }
        }
        None
    }
}

/// 从反编译源码头部提取 import 信息
///
/// 返回 (精确映射: short_name → fqn, 通配符包列表)
fn parse_imports(source: &str) -> (HashMap<String, String>, Vec<String>) {
    let mut exact = HashMap::new();
    let mut wildcards = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("package ") {
            continue;
        }
        if !trimmed.starts_with("import ") {
            // import 段结束
            break;
        }
        // "import static com.example.Foo.BAR;" → 跳过 static import
        let rest = trimmed.strip_prefix("import ").unwrap();
        if rest.starts_with("static ") {
            continue;
        }
        let path = rest.trim_end_matches(';').trim();
        if path.ends_with(".*") {
            wildcards.push(path[..path.len() - 2].to_string());
        } else if let Some(simple) = path.rsplit('.').next() {
            exact.insert(simple.to_string(), path.to_string());
        }
    }
    (exact, wildcards)
}

/// 从 entry_path 推导包名
///
/// "com/example/MyClass.class" → Some("com.example")
fn package_from_entry(entry_path: &str) -> Option<String> {
    let stem = entry_path.strip_suffix(".class")?;
    let last_slash = stem.rfind('/')?;
    Some(stem[..last_slash].replace('/', "."))
}

/// 基本类型和常见装箱类型（不值得跳转）
fn is_primitive_or_boxed(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "long"
            | "short"
            | "byte"
            | "float"
            | "double"
            | "boolean"
            | "char"
            | "void"
            | "String"
            | "Object"
            | "Integer"
            | "Long"
            | "Short"
            | "Byte"
            | "Float"
            | "Double"
            | "Boolean"
            | "Character"
            | "Void"
            | "Class"
            | "Number"
    )
}

/// 在目标反编译源码中搜索声明行
///
/// 返回 0-based 行号
pub fn find_declaration_line(
    source: &str,
    name: &str,
    kind: egui_editor::TokenKind,
) -> Option<usize> {
    match kind {
        egui_editor::TokenKind::Type => find_class_decl(source, name),
        egui_editor::TokenKind::MethodCall | egui_editor::TokenKind::MethodDeclaration => {
            find_method_decl(source, name)
        }
        egui_editor::TokenKind::Constant => {
            find_field_decl(source, name).or_else(|| find_enum_constant(source, name))
        }
        _ => None,
    }
}

/// 搜索 class / interface / enum / record 声明
fn find_class_decl(source: &str, name: &str) -> Option<usize> {
    let patterns = ["class ", "interface ", "enum ", "record "];
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        for pat in &patterns {
            if let Some(pos) = trimmed.find(pat) {
                let after = &trimmed[pos + pat.len()..];
                let ident = after
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()?;
                if ident == name {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// 搜索方法声明（含方法名 + "("，且行内有返回类型或 void 等声明关键字）
fn find_method_decl(source: &str, name: &str) -> Option<usize> {
    let name = name.trim_matches('`');
    let needles = [format!("{name}("), format!("`{name}`(")];
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if !needles.iter().any(|needle| trimmed.contains(needle)) {
            continue;
        }
        // 排除纯调用：声明行通常含返回类型关键字或是构造器
        if is_declaration_line(trimmed) {
            return Some(i);
        }
    }
    None
}

/// 搜索字段声明
fn find_field_decl(source: &str, name: &str) -> Option<usize> {
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // 字段声明模式：TypeName fieldName; 或 TypeName fieldName =
        if (trimmed.contains(&format!(" {name};")) || trimmed.contains(&format!(" {name} =")))
            && !trimmed.contains('(')
            && is_member_line(trimmed)
        {
            return Some(i);
        }
    }
    None
}

/// 搜索 enum 常量
fn find_enum_constant(source: &str, name: &str) -> Option<usize> {
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(name)
            && trimmed[name.len()..]
                .starts_with(|c: char| c == ',' || c == '(' || c == ';' || c.is_whitespace())
        {
            return Some(i);
        }
    }
    None
}

/// 判断该行是否像一个声明行（含访问修饰符或返回类型关键字）
fn is_declaration_line(line: &str) -> bool {
    let keywords = [
        "fun",
        "constructor",
        "public",
        "private",
        "protected",
        "static",
        "final",
        "abstract",
        "synchronized",
        "native",
        "void",
        "int",
        "long",
        "short",
        "byte",
        "float",
        "double",
        "boolean",
        "char",
    ];
    keywords.iter().any(|kw| line.contains(kw))
}

/// 判断该行是否像一个成员声明（字段/常量）
fn is_member_line(line: &str) -> bool {
    let keywords = [
        "public",
        "private",
        "protected",
        "static",
        "final",
        "transient",
        "volatile",
    ];
    keywords.iter().any(|kw| line.contains(kw))
}

/// 在当前源码中尝试推断方法/字段调用者的类型
///
/// 分析 `receiver.member` 模式中 `receiver` 的类型声明
pub fn infer_receiver_type(source: &str, receiver: &str) -> Option<String> {
    if receiver == "this" || receiver == "super" {
        return None;
    }
    // 搜索变量声明：TypeName variableName（= | ; | ,）
    let patterns = [
        format!(" {receiver};"),
        format!(" {receiver} ="),
        format!(" {receiver},"),
        format!(" {receiver})"),
    ];
    for line in source.lines() {
        let trimmed = line.trim();
        for pat in &patterns {
            if let Some(pos) = trimmed.find(pat.as_str()) {
                // 从匹配位置往前找类型名
                let before = trimmed[..pos].trim();
                let type_name = before.split_whitespace().last()?;
                // 过滤掉关键字和非类型标识符
                if !type_name.is_empty()
                    && type_name.chars().next().unwrap().is_uppercase()
                    && type_name
                        .chars()
                        .all(|c: char| c.is_alphanumeric() || c == '_' || c == '.')
                {
                    return Some(type_name.to_string());
                }
            }
        }
    }
    None
}

// ─── App 层导航处理 ───

use super::App;
use egui_editor::TokenKind;
use pervius_java_bridge::decompiler;

impl App {
    /// 处理 Ctrl+Click 导航请求
    pub(crate) fn handle_pending_navigation(&mut self) {
        let Some(nav) = self.layout.editor.pending_navigate.take() else {
            return;
        };
        // 声明处 Ctrl+Click → Find Usages（打开搜索面板并填入 token）
        if nav.hit.is_declaration {
            self.layout.search.open_with_query(&nav.hit.token);
            return;
        }
        let Some(loaded) = self.workspace.loaded() else {
            return;
        };
        let hit = &nav.hit;
        let source_entry = nav.source_entry.as_deref().unwrap_or("");
        // 解析目标 entry_path
        let target_entry = match hit.kind {
            TokenKind::Type => {
                loaded
                    .class_resolver
                    .resolve(&hit.token, &nav.source_text, source_entry)
            }
            TokenKind::MethodCall | TokenKind::Constant => {
                // 有 receiver → 推断 receiver 类型 → 解析类名
                if let Some(receiver) = &hit.receiver {
                    // receiver 首字母大写 → 可能是静态调用（ClassName.method()）
                    if receiver.starts_with(|c: char| c.is_uppercase()) {
                        loaded
                            .class_resolver
                            .resolve(receiver, &nav.source_text, source_entry)
                    } else {
                        // 变量调用 → 推断变量类型
                        infer_receiver_type(&nav.source_text, receiver).and_then(|type_name| {
                            loaded.class_resolver.resolve(
                                &type_name,
                                &nav.source_text,
                                source_entry,
                            )
                        })
                    }
                } else {
                    // 无 receiver → 当前文件内的方法/字段，尝试在当前 tab 内定位
                    if let Some(entry) = &nav.source_entry {
                        let line = find_declaration_line(&nav.source_text, &hit.token, hit.kind);
                        if line.is_some() {
                            self.layout.editor.focus_tab_at(entry, line);
                        }
                    }
                    return;
                }
            }
            _ => return,
        };
        let Some(target) = target_entry else {
            return;
        };
        // 跳过跳转到自身
        if nav.source_entry.as_deref() == Some(&target) {
            let line = find_declaration_line(&nav.source_text, &hit.token, hit.kind);
            if line.is_some() {
                self.layout.editor.focus_tab_at(&target, line);
            }
            return;
        }
        // 尝试在目标源码中定位声明行
        let target_line = self.find_target_line(&target, &hit.token, hit.kind);
        // 跳转（复用搜索结果的 open 模式）
        if !self.layout.editor.focus_tab_at(&target, target_line) {
            let path = target.clone();
            self.layout.file_panel.pending_open = Some(target);
            self.handle_pending_open();
            self.layout.editor.focus_tab_at(&path, target_line);
        }
    }

    /// 在目标文件的反编译源码中查找声明行
    fn find_target_line(&self, entry_path: &str, name: &str, kind: TokenKind) -> Option<usize> {
        let loaded = self.workspace.loaded()?;
        // 优先从磁盘缓存读取反编译源码
        let source = decompiler::cached_source(&loaded.jar.hash, entry_path)?;
        find_declaration_line(&source.source, name, kind)
    }
}
