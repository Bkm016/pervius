//! Java 子进程管理：构建、启动、终止
//!
//! 统一 java 定位、`-jar` 参数、Windows `CREATE_NO_WINDOW` 和管道配置，
//! 所有 Java 外部工具（Vineflower、ClassForge 等）通过 `JavaCommand` 启动。
//!
//! @author sky

use crate::error::BridgeError;
#[cfg(windows)]
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;

/// 用户设置的自定义 java_home（优先于环境变量）
static CUSTOM_JAVA_HOME: Mutex<Option<String>> = Mutex::new(None);

/// 设置自定义 JAVA_HOME 路径（由应用启动时调用）
pub fn set_java_home(path: &str) {
    let mut lock = CUSTOM_JAVA_HOME.lock().unwrap_or_else(|p| p.into_inner());
    *lock = if path.is_empty() {
        None
    } else {
        Some(path.to_owned())
    };
}

/// 将用户输入的路径解析为 java 可执行文件
///
/// 支持三种输入：
/// - JDK 根目录（如 `C:\Program Files\Java\jdk-21`）→ 追加 `bin/java`
/// - bin 目录（如 `C:\Program Files\Java\jdk-21\bin`）→ 追加 `java`
/// - 直接指向 java 可执行文件（如 `.../bin/java.exe`）→ 直接使用
fn resolve_java_exe(root: &Path) -> PathBuf {
    let exe_name = if cfg!(windows) { "java.exe" } else { "java" };
    // 用户直接指向了 java 可执行文件
    if root
        .file_name()
        .is_some_and(|n| n == "java" || n == "java.exe")
        && root.is_file()
    {
        return root.to_path_buf();
    }
    // 指向 bin 目录（不含 bin 后缀的子目录拼接）
    if root
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
    {
        return root.join(exe_name);
    }
    // JDK 根目录
    root.join("bin").join(exe_name)
}

/// 按给定配置解析 java 可执行文件（空值表示使用系统 JAVA_HOME / PATH）。
pub fn resolve_java_path(configured: &str) -> Result<PathBuf, BridgeError> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let java = resolve_java_exe(Path::new(configured));
        if java.exists() {
            return Ok(java);
        }
        return Err(BridgeError::JavaNotFound(java));
    }
    // 系统环境变量
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let java = resolve_java_exe(Path::new(&java_home));
        if java.exists() {
            return Ok(java);
        }
    }
    // PATH 中探测
    find_java_in_path()
}

/// 定位 java 可执行文件
///
/// 查找顺序：
/// 1. 用户设置的自定义路径
/// 2. 系统 JAVA_HOME 环境变量
/// 3. PATH 中探测（`where java` / `which java`）
pub fn find_java() -> Result<PathBuf, BridgeError> {
    let custom = CUSTOM_JAVA_HOME
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_default();
    resolve_java_path(&custom)
}

/// 通过 PATH 环境变量查找 java
fn find_java_in_path() -> Result<PathBuf, BridgeError> {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    let mut command = Command::new(cmd);
    command.arg("java");
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = command.output();
    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Some(first) = stdout.lines().next() {
                let path = PathBuf::from(first.trim());
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }
    Err(BridgeError::JavaHomeNotSet)
}

tabookit::class! {
    /// Java `-jar` 子进程构建器
    ///
    /// 自动处理 java 定位、Windows `CREATE_NO_WINDOW`、管道配置。
    pub struct JavaCommand {
        cmd: Command,
    }
    /// 创建 `java -jar <jar>` 命令
    pub fn new(jar: &Path) -> Result<Self, BridgeError> {
        let java = find_java()?;
        let mut cmd = Command::new(java);
        cmd.arg("-jar").arg(jar);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        Ok(Self { cmd })
    }

    /// 创建 `java -cp <jars> <main-class>` 命令（多 JAR 使用平台分隔符）
    pub fn with_classpath(jars: &[PathBuf], main_class: &str) -> Result<Self, BridgeError> {
        let java = find_java()?;
        let mut cmd = Command::new(java);
        let sep = if cfg!(windows) { ";" } else { ":" };
        let cp = jars
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(sep);
        cmd.arg("-cp").arg(cp).arg(main_class);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        Ok(Self { cmd })
    }

    /// 追加单个参数
    pub fn arg(&mut self, arg: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.cmd.arg(arg);
        self
    }

    /// 同步执行，管道收集 stdout/stderr
    pub fn output(&mut self) -> Result<Output, std::io::Error> {
        self.cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        self.cmd.output()
    }

    /// 启动子进程（stdout/stderr 管道）
    pub fn spawn(&mut self) -> Result<Child, std::io::Error> {
        self.cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        self.cmd.spawn()
    }

    /// 启动子进程（stdin/stdout/stderr 管道）
    pub fn spawn_with_stdin(&mut self) -> Result<Child, std::io::Error> {
        self.cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.cmd.spawn()
    }
}

/// 终止进程及其子进程树
#[cfg(windows)]
pub fn kill_tree(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let _ = cmd.output();
}

/// 终止进程及其子进程树
#[cfg(not(windows))]
pub fn kill_tree(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).output();
}
