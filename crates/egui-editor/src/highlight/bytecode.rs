//! JVM 字节码着色
//!
//! 行级 + 词级解析，无 tree-sitter 依赖。
//! 支持 Recaf 风格反汇编输出（大写操作码 + 内联 CP 引用）。
//!
//! @author sky

use super::{Span, TokenKind};
use phf::phf_set;

/// JVM 操作码集合（全大写，phf 完美哈希，O(1) 查找）
static OPCODES: phf::Set<&'static str> = phf_set! {
    "AALOAD", "AASTORE", "ACONST_NULL",
    "ALOAD", "ALOAD_0", "ALOAD_1", "ALOAD_2", "ALOAD_3", "ALOAD_W",
    "ANEWARRAY", "ARETURN", "ARRAYLENGTH",
    "ASTORE", "ASTORE_0", "ASTORE_1", "ASTORE_2", "ASTORE_3", "ASTORE_W",
    "ATHROW",
    "BALOAD", "BASTORE", "BIPUSH", "BREAKPOINT",
    "CALOAD", "CASTORE", "CHECKCAST",
    "D2F", "D2I", "D2L", "DADD", "DALOAD", "DASTORE",
    "DCMPG", "DCMPL", "DCONST_0", "DCONST_1",
    "DDIV", "DLOAD", "DLOAD_0", "DLOAD_1", "DLOAD_2", "DLOAD_3", "DLOAD_W",
    "DMUL", "DNEG", "DREM", "DRETURN",
    "DSTORE", "DSTORE_0", "DSTORE_1", "DSTORE_2", "DSTORE_3", "DSTORE_W",
    "DSUB", "DUP", "DUP2", "DUP2_X1", "DUP2_X2", "DUP_X1", "DUP_X2",
    "F2D", "F2I", "F2L", "FADD", "FALOAD", "FASTORE",
    "FCMPG", "FCMPL", "FCONST_0", "FCONST_1", "FCONST_2",
    "FDIV", "FLOAD", "FLOAD_0", "FLOAD_1", "FLOAD_2", "FLOAD_3", "FLOAD_W",
    "FMUL", "FNEG", "FREM", "FRETURN",
    "FSTORE", "FSTORE_0", "FSTORE_1", "FSTORE_2", "FSTORE_3", "FSTORE_W",
    "FSUB",
    "GETFIELD", "GETSTATIC", "GOTO", "GOTO_W",
    "I2B", "I2C", "I2D", "I2F", "I2L", "I2S",
    "IADD", "IALOAD", "IAND", "IASTORE",
    "ICONST_0", "ICONST_1", "ICONST_2", "ICONST_3", "ICONST_4", "ICONST_5", "ICONST_M1",
    "IDIV", "IFEQ", "IFGE", "IFGT", "IFLE", "IFLT", "IFNE",
    "IFNONNULL", "IFNULL",
    "IF_ACMPEQ", "IF_ACMPNE",
    "IF_ICMPEQ", "IF_ICMPGE", "IF_ICMPGT", "IF_ICMPLE", "IF_ICMPLT", "IF_ICMPNE",
    "IINC", "IINC_W",
    "ILOAD", "ILOAD_0", "ILOAD_1", "ILOAD_2", "ILOAD_3", "ILOAD_W",
    "IMPDEP1", "IMPDEP2", "IMUL", "INEG",
    "INSTANCEOF", "INVOKEDYNAMIC", "INVOKEINTERFACE", "INVOKESPECIAL", "INVOKESTATIC", "INVOKEVIRTUAL",
    "IOR", "IREM", "IRETURN", "ISHL", "ISHR",
    "ISTORE", "ISTORE_0", "ISTORE_1", "ISTORE_2", "ISTORE_3", "ISTORE_W",
    "ISUB", "IUSHR", "IXOR",
    "JSR", "JSR_W",
    "L2D", "L2F", "L2I", "LADD", "LALOAD", "LAND", "LASTORE", "LCMP",
    "LCONST_0", "LCONST_1", "LDC", "LDC2_W", "LDC_W",
    "LDIV", "LLOAD", "LLOAD_0", "LLOAD_1", "LLOAD_2", "LLOAD_3", "LLOAD_W",
    "LMUL", "LNEG", "LOOKUPSWITCH",
    "LOR", "LREM", "LRETURN", "LSHL", "LSHR",
    "LSTORE", "LSTORE_0", "LSTORE_1", "LSTORE_2", "LSTORE_3", "LSTORE_W",
    "LSUB", "LUSHR", "LXOR",
    "MONITORENTER", "MONITOREXIT", "MULTIANEWARRAY",
    "NEW", "NEWARRAY", "NOP",
    "POP", "POP2", "PUTFIELD", "PUTSTATIC",
    "RET", "RET_W", "RETURN",
    "SALOAD", "SASTORE", "SIPUSH", "SWAP",
    "TABLESWITCH", "WIDE",
};

/// 访问修饰符和声明关键字（全小写）
static KEYWORDS: phf::Set<&'static str> = phf_set! {
    "abstract", "any", "bridge", "class", "default", "define", "enum",
    "extends", "final", "implements", "interface", "line",
    "native", "private", "protected", "public", "static",
    "strictfp", "synchronized", "synthetic", "transient", "varargs", "volatile",
};

/// 栈上 ASCII 缓冲区，避免 to_ascii_uppercase/lowercase 的堆分配
///
/// JVM 操作码最长 16 字符（INVOKEINTERFACE），预留 24 字节足够。
const WORD_BUF_LEN: usize = 24;

/// 大小写不敏感地查找操作码（零堆分配）
fn is_opcode(word: &str) -> bool {
    if word.len() > WORD_BUF_LEN || !word.is_ascii() {
        return false;
    }
    let mut buf = [0u8; WORD_BUF_LEN];
    buf[..word.len()].copy_from_slice(word.as_bytes());
    buf[..word.len()].make_ascii_uppercase();
    // SAFETY: 输入是 ASCII，大写转换后仍是合法 UTF-8
    let upper = unsafe { std::str::from_utf8_unchecked(&buf[..word.len()]) };
    OPCODES.contains(upper)
}

/// 大小写不敏感地查找关键字（零堆分配）
fn is_keyword(word: &str) -> bool {
    if word.len() > WORD_BUF_LEN || !word.is_ascii() {
        return false;
    }
    let mut buf = [0u8; WORD_BUF_LEN];
    buf[..word.len()].copy_from_slice(word.as_bytes());
    buf[..word.len()].make_ascii_lowercase();
    let lower = unsafe { std::str::from_utf8_unchecked(&buf[..word.len()]) };
    KEYWORDS.contains(lower)
}

pub fn collect_spans(source: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for line in source.split('\n') {
        tokenize_line(line, offset, &mut spans);
        offset += line.len() + 1;
    }
    spans
}

/// 单行 tokenize 入口：按行前缀分派到专用路径
fn tokenize_line(line: &str, base: usize, spans: &mut Vec<Span>) {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return;
    }
    // 整行注释
    if trimmed.starts_with("//") {
        let indent = line.len() - trimmed.len();
        spans.push((base + indent, base + line.len(), TokenKind::Comment));
        return;
    }
    // 异常表
    if trimmed == "Exception table:" || trimmed.starts_with("from ") || is_exception_data(trimmed) {
        let indent = line.len() - trimmed.len();
        spans.push((base + indent, base + line.len(), TokenKind::Muted));
        return;
    }
    // MAXSTACK / MAXLOCALS
    if trimmed.starts_with("MAXSTACK") || trimmed.starts_with("MAXLOCALS") {
        tokenize_meta(line, base, spans);
        return;
    }
    // 普通指令行
    tokenize_instruction(line, base, spans);
}

/// 判断是否为异常表数据行
///
/// 格式: `0  8  9  java/io/IOException` 或 `0  8  9  any`
fn is_exception_data(trimmed: &str) -> bool {
    let mut parts = trimmed.split_ascii_whitespace();
    let first = match parts.next() {
        Some(p) => p,
        None => return false,
    };
    first.bytes().all(|b| b.is_ascii_digit()) && parts.count() >= 2
}

/// MAXSTACK / MAXLOCALS 行：关键字 → Muted，= → Muted，数字 → Number
fn tokenize_meta(line: &str, base: usize, spans: &mut Vec<Span>) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = skip_ws(bytes, 0);
    while i < len {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i = skip_ws(bytes, i);
            continue;
        }
        if b == b'=' {
            spans.push((base + i, base + i + 1, TokenKind::Muted));
            i += 1;
            continue;
        }
        if b.is_ascii_digit() {
            let start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            spans.push((base + start, base + i, TokenKind::Number));
            continue;
        }
        let start = i;
        while i < len && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i > start {
            spans.push((base + start, base + i, TokenKind::Muted));
        }
    }
}

/// 普通指令行 tokenize
///
/// 可选前缀 `数字:` 作为指令索引（Muted），然后逐 token 分类。
fn tokenize_instruction(line: &str, base: usize, spans: &mut Vec<Span>) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = skip_ws(bytes, 0);
    // 指令索引前缀：digits + ':'
    if i < len && bytes[i].is_ascii_digit() {
        let digit_start = i;
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < len && bytes[i] == b':' {
            spans.push((base + digit_start, base + i + 1, TokenKind::Muted));
            i += 1;
        } else {
            i = digit_start;
        }
    }
    // 主循环
    while i < len {
        let b = bytes[i];
        // 内联注释
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            spans.push((base + i, base + len, TokenKind::Comment));
            return;
        }
        if b.is_ascii_whitespace() {
            i = skip_ws(bytes, i);
            continue;
        }
        // 字符串 "..."
        if b == b'"' {
            i = scan_string(line, i, base, spans);
            continue;
        }
        // 标点
        if is_punctuation(b) {
            spans.push((base + i, base + i + 1, TokenKind::Muted));
            i += 1;
            continue;
        }
        // 单词（含 / . $ # - 等组成的标识符路径）
        let start = i;
        while i < len && !is_token_break(bytes[i]) {
            i += 1;
        }
        if i > start {
            let word = &line[start..i];
            spans.push((base + start, base + i, classify_word(word)));
        }
    }
}

/// 扫描双引号字符串，处理转义
fn scan_string(line: &str, start: usize, base: usize, spans: &mut Vec<Span>) -> usize {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = start + 1;
    while i < len && bytes[i] != b'"' {
        if bytes[i] == b'\\' {
            i += 1;
        }
        i += 1;
    }
    if i < len {
        i += 1; // 闭合引号
    }
    spans.push((base + start, base + i, TokenKind::String));
    i
}

/// 跳过 ASCII 空白，返回新位置
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn is_punctuation(b: u8) -> bool {
    matches!(b, b'{' | b'}' | b'(' | b')' | b';' | b'=' | b',' | b':')
}

fn is_token_break(b: u8) -> bool {
    b.is_ascii_whitespace() || is_punctuation(b) || b == b'"'
}

/// 单词分类
fn classify_word(word: &str) -> TokenKind {
    if is_opcode(word) {
        return TokenKind::Opcode;
    }
    if is_keyword(word) {
        return TokenKind::Keyword;
    }
    // 方法特殊名
    if word == "<init>" || word == "<clinit>" {
        return TokenKind::MethodDeclaration;
    }
    // #常量池引用
    if word.starts_with('#') && word.len() > 1 && word[1..].bytes().all(|b| b.is_ascii_digit()) {
        return TokenKind::Number;
    }
    // 数字
    if is_number(word) {
        return TokenKind::Number;
    }
    // JVM 类型描述符（L...;、[I、V 等）
    if is_descriptor(word) {
        return TokenKind::Type;
    }
    // 内部名称路径（java/lang/Object）
    if is_internal_name(word) {
        return TokenKind::Type;
    }
    // 成员引用（Owner.member，必须同时含 / 和 .）
    if word.contains('.') && word.contains('/') {
        return TokenKind::Type;
    }
    // 标签（L0, L1, ...）
    if word.starts_with('L') && word.len() > 1 && word[1..].bytes().all(|b| b.is_ascii_digit()) {
        return TokenKind::Number;
    }
    TokenKind::Plain
}

fn is_number(word: &str) -> bool {
    if word.starts_with("0x") || word.starts_with("0X") {
        return word.len() > 2 && word[2..].bytes().all(|b| b.is_ascii_hexdigit());
    }
    let s = word.strip_prefix('-').unwrap_or(word);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// JVM 类型描述符
///
/// 这里故意不把单字符原始类型（`I`/`D`/`V` 等）直接当成 Type，
/// 因为 Recaf 风格字节码里分支标签和 `.var` 范围标签常用单个大写字母，
/// 否则会把 `A B C D ...` 这类标签误着色成青色。
///
/// 仍保留对象描述符（`L...;`）和数组描述符（`[` 后接有效描述符前缀）。
fn is_descriptor(word: &str) -> bool {
    let b = word.as_bytes();
    if b.is_empty() {
        return false;
    }
    // L...;  对象类型
    if b[0] == b'L' && *b.last().unwrap() == b';' && b.len() > 2 {
        return true;
    }
    // [开头 + 有效描述符后缀（原始类型字符或 L 或嵌套 [）
    if b[0] == b'[' && b.len() > 1 {
        return matches!(
            b[1],
            b'V' | b'Z' | b'B' | b'C' | b'S' | b'I' | b'J' | b'F' | b'D' | b'L' | b'['
        );
    }
    false
}

/// 内部名称路径（java/lang/Object）
///
/// 至少含一个 `/` 分隔的两段，每段仅 ASCII 字母数字 + `$` + `_`。
fn is_internal_name(word: &str) -> bool {
    if !word.contains('/') {
        return false;
    }
    let mut segments = 0u32;
    for part in word.split('/') {
        if part.is_empty() {
            return false;
        }
        if !part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'$' || b == b'_')
        {
            return false;
        }
        segments += 1;
    }
    segments >= 2
}
