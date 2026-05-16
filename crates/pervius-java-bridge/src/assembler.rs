//! ClassForge 集成：调用 ASM 处理字节码修改、StackMapTable 生成、max 值计算
//!
//! 通过 `patch_methods` 发送 class 字节 + 方法编辑列表，由 ASM 替换字节码并重算帧
//!
//! @author sky

use crate::error::BridgeError;
use std::io::Write;
use std::path::Path;

use crate::process;

/// 获取 ClassForge 版本号（从 JAR 文件名解析）
///
/// 返回版本字符串（如 `"1.0.0"`），未找到时返回 `None`。
/// 调用方负责 i18n 格式化。
pub fn classforge_version() -> Option<String> {
    let path = crate::find_jar(
        "classforge",
        |_| true,
        Some((crate::BUNDLED_CLASSFORGE, crate::BUNDLED_CLASSFORGE_NAME)),
    )
    .ok()?;
    crate::jar_version("classforge", &path)
}

/// 方法字节码编辑
pub struct MethodEdit {
    /// 方法名
    pub name: String,
    /// 方法描述符
    pub descriptor: String,
    /// 新字节码文本（Pervius 格式）
    pub bytecode: String,
}

/// 调用 classforge --patch 模式：替换方法字节码 + 重算帧/max 值
///
/// ASM 接管常量池管理、指令编码、StackMapTable 生成、max_stack/max_locals 计算。
/// Rust 侧无需维护 CP，只传字节码文本。
///
/// 协议（stdin, big-endian）：
///   [4B class 长度][class 字节]
///   [4B 编辑数]
///   每条：[2B name 长度][name][2B desc 长度][desc][4B code 长度][code 文本]
pub fn patch_methods(
    class_bytes: &[u8],
    edits: &[MethodEdit],
    jar_path: Option<&Path>,
) -> Result<Vec<u8>, BridgeError> {
    let classforge = crate::find_jar(
        "classforge",
        |_| true,
        Some((crate::BUNDLED_CLASSFORGE, crate::BUNDLED_CLASSFORGE_NAME)),
    )?;
    let mut cmd = process::JavaCommand::new(&classforge)?;
    cmd.arg("--patch");
    if let Some(path) = jar_path {
        cmd.arg("--classpath").arg(path);
    }
    let mut child = cmd.spawn_with_stdin().map_err(BridgeError::SpawnFailed)?;
    if let Some(mut stdin) = child.stdin.take() {
        // class 数据
        stdin.write_all(&(class_bytes.len() as u32).to_be_bytes())?;
        stdin.write_all(class_bytes)?;
        // 编辑数
        stdin.write_all(&(edits.len() as u32).to_be_bytes())?;
        for edit in edits {
            write_prefixed_string(&mut stdin, &edit.name)?;
            write_prefixed_string(&mut stdin, &edit.descriptor)?;
            write_prefixed_string_u32(&mut stdin, &edit.bytecode)?;
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BridgeError::ClassForge(stderr.into_owned()));
    }
    if output.stdout.is_empty() {
        return Err(BridgeError::ClassForge("produced no output".to_string()));
    }
    // 校验输出是合法的 class 文件（防止 stdout 混入日志或 JVM 警告）
    if output.stdout.len() < 4 || output.stdout[..4] != [0xCA, 0xFE, 0xBA, 0xBE] {
        return Err(BridgeError::ClassForge(
            "output is not a valid class file (missing CAFEBABE magic)".to_string(),
        ));
    }
    log::debug!(
        "classforge patch: {} -> {} bytes ({} methods patched)",
        class_bytes.len(),
        output.stdout.len(),
        edits.len()
    );
    Ok(output.stdout)
}

/// 写入 u16 长度前缀的 UTF-8 字符串
fn write_prefixed_string(w: &mut dyn Write, s: &str) -> Result<(), BridgeError> {
    let bytes = s.as_bytes();
    w.write_all(&(bytes.len() as u16).to_be_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}

/// 写入 u32 长度前缀的 UTF-8 字符串（用于较长的字节码文本）
fn write_prefixed_string_u32(w: &mut dyn Write, s: &str) -> Result<(), BridgeError> {
    let bytes = s.as_bytes();
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}
