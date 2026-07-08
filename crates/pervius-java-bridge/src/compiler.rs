//! ClassForge 源码编译 bridge：调用 JDK javax.tools.JavaCompiler
//!
//! @author sky

use crate::error::BridgeError;
use crate::process;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::process::Child;

/// Kotlin 源文件输入
#[derive(Clone, Debug)]
pub struct KotlinSource {
    /// 相对文件路径，如 `com/example/Foo.kt`
    pub path: String,
    /// Kotlin 源码
    pub source: String,
}

/// 编译 classpath 条目
#[derive(Clone, Debug, Default)]
pub struct CompileClasspath {
    /// 参与编译的 JAR / ZIP / 目录条目，按顺序传递给编译器。
    pub entries: Vec<PathBuf>,
}

impl CompileClasspath {
    /// 创建空 classpath。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个条目。
    pub fn push(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        if !path.as_os_str().is_empty() && !self.entries.iter().any(|p| p == &path) {
            self.entries.push(path);
        }
    }

    /// 按平台分隔符拼接为 Java classpath 字符串。
    pub fn joined(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let sep = if cfg!(windows) { ";" } else { ":" };
        Some(
            self.entries
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(sep),
        )
    }
}

/// 编译产物 class
#[derive(Clone, Debug)]
pub struct CompiledClass {
    /// 斜杠形式 binary name，如 `com/foo/Bar$Inner`
    pub binary_name: String,
    /// class 文件字节
    pub bytes: Vec<u8>,
}

/// 编译诊断等级
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagSeverity {
    Error,
    Warning,
    Note,
}

/// 编译诊断
#[derive(Clone, Debug)]
pub struct CompileDiagnostic {
    pub severity: DiagSeverity,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// 编译结果
#[derive(Clone, Debug)]
pub enum CompileOutcome {
    Success(Vec<CompiledClass>),
    Errors(Vec<CompileDiagnostic>),
    JdkMissing,
}

/// 当前 Java 配置是否能找到同目录 javac
pub fn is_jdk_available() -> bool {
    let Ok(java) = process::find_java() else {
        return false;
    };
    let Some(bin) = java.parent() else {
        return false;
    };
    let javac = bin.join(if cfg!(windows) { "javac.exe" } else { "javac" });
    javac.exists()
}

fn classforge_jar() -> Result<PathBuf, BridgeError> {
    crate::find_jar(
        "classforge",
        |_| true,
        Some((crate::BUNDLED_CLASSFORGE, crate::BUNDLED_CLASSFORGE_NAME)),
    )
}

fn wait_compile_output(child: Child) -> Result<CompileOutcome, BridgeError> {
    let output = child.wait_with_output()?;
    if output.stdout.first() == Some(&2) {
        return Ok(CompileOutcome::JdkMissing);
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BridgeError::Compile(stderr.into_owned()));
    }
    parse_compile_output(&output.stdout)
}

/// 调用 classforge --compile 编译 Java 源码
pub fn compile_source(
    source: &str,
    binary_name: &str,
    classpath: &CompileClasspath,
    target: Option<u8>,
    debug: bool,
) -> Result<CompileOutcome, BridgeError> {
    let classforge = classforge_jar()?;
    let mut cmd = process::JavaCommand::new(&classforge)?;
    cmd.arg("--compile");
    if let Some(classpath) = classpath.joined() {
        cmd.arg("--classpath").arg(classpath);
    }
    if let Some(target) = target {
        cmd.arg("--target").arg(target.to_string());
    }
    if debug {
        cmd.arg("--debug");
    }
    let mut child = cmd.spawn_with_stdin().map_err(BridgeError::SpawnFailed)?;
    if let Some(mut stdin) = child.stdin.take() {
        write_prefixed_string(&mut stdin, binary_name)?;
        write_prefixed_string_u32(&mut stdin, source)?;
    }
    wait_compile_output(child)
}

/// 调用 classforge --compile-kt 编译 Kotlin 源码（可配置 Kotlin 编译器兼容选项）
pub fn compile_kotlin_sources_with_options(
    sources: &[KotlinSource],
    classpath: &CompileClasspath,
    target: Option<u8>,
    module_name: Option<&str>,
    skip_metadata_version_check: bool,
) -> Result<CompileOutcome, BridgeError> {
    let classforge = classforge_jar()?;
    let kotlin_dependencies = crate::environment::ensure_kotlin_dependencies()?;
    let mut runtime_classpath =
        Vec::with_capacity(2 + kotlin_dependencies.runtime_dependencies.len());
    runtime_classpath.push(classforge.clone());
    runtime_classpath.push(kotlin_dependencies.compiler_embeddable.clone());
    runtime_classpath.extend(kotlin_dependencies.runtime_dependencies.iter().cloned());
    let mut cmd =
        process::JavaCommand::with_classpath(&runtime_classpath, "pervius.asm.ClassForge")?;
    cmd.arg("--compile-kt");
    let mut compile_classpath = classpath.clone();
    compile_classpath.push(kotlin_dependencies.stdlib.clone());
    if let Some(classpath) = compile_classpath.joined() {
        cmd.arg("--classpath").arg(classpath);
    }
    if let Some(target) = target {
        cmd.arg("--target").arg(target.to_string());
    }
    if let Some(module_name) = module_name {
        cmd.arg("--module-name").arg(module_name);
    }
    if !skip_metadata_version_check {
        cmd.arg("--no-skip-metadata-version-check");
    }
    let mut child = cmd.spawn_with_stdin().map_err(BridgeError::SpawnFailed)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&(sources.len() as u32).to_be_bytes())?;
        for source in sources {
            write_prefixed_string(&mut stdin, &source.path)?;
            write_prefixed_string_u32(&mut stdin, &source.source)?;
        }
    }
    wait_compile_output(child)
}

fn parse_compile_output(stdout: &[u8]) -> Result<CompileOutcome, BridgeError> {
    let mut cur = Cursor::new(stdout);
    let status = read_u8(&mut cur)?;
    match status {
        0 => {
            let count = read_u32(&mut cur)? as usize;
            let mut classes = Vec::with_capacity(count);
            for _ in 0..count {
                let binary_name = read_prefixed_string(&mut cur)?;
                let len = read_u32(&mut cur)? as usize;
                let mut bytes = vec![0; len];
                cur.read_exact(&mut bytes)?;
                classes.push(CompiledClass { binary_name, bytes });
            }
            Ok(CompileOutcome::Success(classes))
        }
        1 => {
            let count = read_u32(&mut cur)? as usize;
            let mut diagnostics = Vec::with_capacity(count);
            for _ in 0..count {
                let severity = match read_u8(&mut cur)? {
                    0 => DiagSeverity::Error,
                    1 => DiagSeverity::Warning,
                    _ => DiagSeverity::Note,
                };
                let line = read_u32(&mut cur)?;
                let column = read_u32(&mut cur)?;
                let message = read_prefixed_string_u32(&mut cur)?;
                diagnostics.push(CompileDiagnostic {
                    severity,
                    line,
                    column,
                    message,
                });
            }
            Ok(CompileOutcome::Errors(diagnostics))
        }
        2 => Ok(CompileOutcome::JdkMissing),
        other => Err(BridgeError::Compile(format!(
            "unexpected compiler status byte {other}"
        ))),
    }
}

fn write_prefixed_string(w: &mut dyn Write, s: &str) -> Result<(), BridgeError> {
    let bytes = s.as_bytes();
    if bytes.len() > u16::MAX as usize {
        return Err(BridgeError::Compile("binary name is too long".to_string()));
    }
    w.write_all(&(bytes.len() as u16).to_be_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}

fn write_prefixed_string_u32(w: &mut dyn Write, s: &str) -> Result<(), BridgeError> {
    let bytes = s.as_bytes();
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}

fn read_u8(r: &mut Cursor<&[u8]>) -> Result<u8, BridgeError> {
    let mut buf = [0; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_u32(r: &mut Cursor<&[u8]>) -> Result<u32, BridgeError> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

fn read_prefixed_string(r: &mut Cursor<&[u8]>) -> Result<String, BridgeError> {
    let mut len_buf = [0; 2];
    r.read_exact(&mut len_buf)?;
    let len = u16::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| BridgeError::Compile(e.to_string()))
}

fn read_prefixed_string_u32(r: &mut Cursor<&[u8]>) -> Result<String, BridgeError> {
    let len = read_u32(r)? as usize;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| BridgeError::Compile(e.to_string()))
}
