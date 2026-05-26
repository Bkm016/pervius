//! Java bridge：JAR 归档读取、Vineflower 反编译、字节码反汇编、class 编辑写回
//!
//! 提供与 Java 工具链交互的全部逻辑，无 UI 依赖。
//!
//! @author sky

use std::path::PathBuf;

#[macro_use]
mod macros;

pub mod assembler;
pub mod bytecode;
pub mod class_structure;
pub mod compiler;
pub mod decompiler;
pub(crate) mod deps;
pub mod error;
pub mod jar;
pub mod process;
pub mod environment;
pub mod save;

/// Kotlin 编译器生成的内部注解，不可编辑，读写时均跳过
pub const KOTLIN_INTERNAL_ANNOTATIONS: &[&str] = &[
    "Lkotlin/Metadata;",
    "Lkotlin/jvm/internal/SourceDebugExtension;",
];

/// 内置 ClassForge JAR（编译时嵌入）
const BUNDLED_CLASSFORGE: &[u8] = include_bytes!("../libs/classforge-1.1.jar");
/// 内置 ClassForge 文件名
const BUNDLED_CLASSFORGE_NAME: &str = "classforge-1.1.jar";

/// 内置 JAR 释放目标目录（`<data_dir>/pervius/libs/`）
fn bundled_libs_dir() -> Result<PathBuf, error::BridgeError> {
    let base = dirs::data_dir().ok_or(error::BridgeError::NoCacheDir)?;
    Ok(base.join("pervius").join("libs"))
}

/// 释放内置 JAR 到数据目录（已存在且大小一致则跳过）
fn extract_bundled_jar(data: &[u8], filename: &str) -> Result<PathBuf, error::BridgeError> {
    let dir = bundled_libs_dir()?;
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(filename);
    // 已存在且大小一致 → 跳过写入
    if let Ok(meta) = std::fs::metadata(&target) {
        if meta.len() == data.len() as u64 {
            return Ok(target);
        }
    }
    std::fs::write(&target, data)?;
    log::info!("Extracted bundled {filename} to {}", target.display());
    Ok(target)
}

/// 查找 JAR 文件
///
/// 搜索顺序：
/// 1. exe 同目录（用户覆盖）
/// 2. 有内置 JAR 时，使用数据目录中的指定内置版本，缺失则释放
/// 3. 无内置 JAR 时，探测数据目录中的同名前缀 JAR
///
/// `prefix` — 文件名前缀（如 `"vineflower"`）
/// `filter` — 额外过滤条件（如排除 `-slim`），无需额外过滤时传 `|_| true`
/// `bundled` — 内置 JAR 数据和文件名，无内置时传 `None`
pub fn find_jar(
    prefix: &str,
    filter: impl Fn(&str) -> bool,
    bundled: Option<(&[u8], &str)>,
) -> Result<PathBuf, error::BridgeError> {
    // 1. exe 同目录探测
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            if let Ok(entries) = std::fs::read_dir(exe_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with(prefix) && name.ends_with(".jar") && filter(&name) {
                        return Ok(entry.path());
                    }
                }
            }
        }
    }
    // 2. 内置 JAR 有明确文件名时优先保证该版本存在，避免数据目录里的旧版同名前缀 JAR 抢先命中。
    if let Some((data, filename)) = bundled {
        return extract_bundled_jar(data, filename);
    }
    // 3. 数据目录探测（无内置 JAR 的外部依赖，如 kotlinc）
    if let Ok(dir) = bundled_libs_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(prefix) && name.ends_with(".jar") && filter(&name) {
                    return Ok(entry.path());
                }
            }
        }
    }
    Err(error::BridgeError::JarNotFound {
        prefix: prefix.to_string(),
    })
}

/// 从 JAR 文件名解析版本号（去掉前缀和 `.jar` 后缀）
///
/// 如 `vineflower-1.11.1.jar` → `Some("1.11.1")`
pub fn jar_version(prefix: &str, path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let version = stem.strip_prefix(prefix)?.strip_prefix('-')?;
    Some(version.to_string())
}
