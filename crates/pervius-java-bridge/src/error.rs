//! 统一错误类型
//!
//! @author sky

use std::path::PathBuf;

/// Java bridge 统一错误类型
#[derive(Debug)]
pub enum BridgeError {
    /// JAVA_HOME 环境变量未设置
    JavaHomeNotSet,
    /// java 可执行文件不存在
    JavaNotFound(PathBuf),
    /// 指定前缀的 JAR 未找到（exe 同目录和数据目录均无匹配）
    JarNotFound {
        /// 搜索的文件名前缀
        prefix: String,
    },
    /// 无法确定系统缓存目录
    NoCacheDir,
    /// 子进程启动失败
    SpawnFailed(std::io::Error),
    /// 外部工具执行失败（stderr 内容）
    ProcessFailed(String),
    /// 外部工具非零退出码
    ExitCode(Option<i32>),
    /// IO 错误
    Io(std::io::Error),
    /// class 文件解析错误
    Parse(String),
    /// ClassForge 工具错误
    ClassForge(String),
    /// 当前 Java 环境缺少 javac
    JdkRequired,
    /// Java 源码编译错误
    Compile(String),
    /// 外部工具下载失败
    Download(String),
    /// Vineflower 未产出结果
    NoOutput,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JavaHomeNotSet => write!(
                f,
                "Cannot find Java: set JAVA_HOME, configure in settings, or add java to PATH"
            ),
            Self::JavaNotFound(path) => {
                write!(f, "Java executable not found at {}", path.display())
            }
            Self::JarNotFound { prefix } => {
                write!(f, "{prefix}*.jar not found")
            }
            Self::NoCacheDir => write!(f, "Cannot determine cache directory"),
            Self::SpawnFailed(e) => write!(f, "Failed to spawn process: {e}"),
            Self::ProcessFailed(stderr) => write!(f, "{stderr}"),
            Self::ExitCode(code) => write!(f, "Process exited with code {code:?}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::ClassForge(msg) => write!(f, "ClassForge: {msg}"),
            Self::JdkRequired => write!(f, "JDK required to compile Java source"),
            Self::Compile(msg) => write!(f, "Compile error: {msg}"),
            Self::Download(msg) => write!(f, "Download error: {msg}"),
            Self::NoOutput => write!(f, "Decompiler produced no output"),
        }
    }
}

impl std::error::Error for BridgeError {}

impl BridgeError {
    /// 快捷构造 Parse 变体
    pub fn parse(e: impl std::fmt::Display) -> Self {
        Self::Parse(e.to_string())
    }
}

impl From<std::io::Error> for BridgeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
