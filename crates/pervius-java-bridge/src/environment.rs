//! 外部 Java 依赖管理：配置同步、默认目录解析与公开依赖定义。
//!
//! 依赖下载、校验与 POM 解析实现在 `crate::deps` 中。
//!
//! @author TheFloodDragon

use crate::decompiler;
use crate::error::BridgeError;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Maven 基础仓库地址。
pub const MAVEN_BASE: &str = "https://repo.huaweicloud.com/repository/maven";

/// 默认 Vineflower 版本
pub const DEFAULT_VINEFLOWER_VERSION: &str = "1.12.0";
/// 默认 Kotlin 版本
pub const DEFAULT_KOTLIN_VERSION: &str = "2.3.0";

/// 已解析的 Kotlin 依赖路径。
#[derive(Clone, Debug)]
pub struct KotlinDependencies {
    /// kotlin-stdlib JAR（同时用于编译 classpath）
    pub stdlib: PathBuf,
    /// kotlin-compiler-embeddable JAR
    pub compiler_embeddable: PathBuf,
    /// Kotlin 编译器运行时依赖（按 POM 声明顺序）
    pub runtime_dependencies: Vec<PathBuf>,
}

/// 当前下载进度快照。
#[derive(Clone, Debug)]
pub struct DownloadProgressSnapshot {
    /// 正在下载的文件名。
    pub file_name: String,
    /// 已下载字节数。
    pub downloaded: u64,
    /// 总字节数；服务器未返回 Content-Length 时为 None。
    pub total: Option<u64>,
}

/// 外部依赖配置（由 UI 设置同步到 bridge 层）。
#[derive(Clone, Debug)]
pub struct EnvironmentConfig {
    /// Vineflower 版本
    pub vineflower_version: String,
    /// Vineflower 存储目录；None 表示默认目录。
    pub vineflower_dir: Option<PathBuf>,
    /// Kotlin 版本
    pub kotlin_version: String,
    /// Kotlin 依赖存储目录；None 表示默认目录。
    pub kotlin_dependencies_dir: Option<PathBuf>,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            vineflower_version: DEFAULT_VINEFLOWER_VERSION.to_string(),
            vineflower_dir: None,
            kotlin_version: DEFAULT_KOTLIN_VERSION.to_string(),
            kotlin_dependencies_dir: None,
        }
    }
}

static ENVIRONMENT_CONFIG: Mutex<EnvironmentConfig> = Mutex::new(EnvironmentConfig {
    vineflower_version: String::new(),
    vineflower_dir: None,
    kotlin_version: String::new(),
    kotlin_dependencies_dir: None,
});

/// 设置外部依赖配置。
pub fn set_environment_config(config: EnvironmentConfig) {
    let mut lock = ENVIRONMENT_CONFIG.lock().unwrap_or_else(|p| p.into_inner());
    *lock = normalize_config(config);
}

/// 获取当前外部依赖配置。
pub fn environment_config() -> EnvironmentConfig {
    let config = ENVIRONMENT_CONFIG
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    normalize_config(config)
}

/// 获取当前下载进度快照。
pub fn download_progress() -> Option<DownloadProgressSnapshot> {
    crate::deps::download_progress()
}

/// 定位并按需下载 Vineflower。
pub fn ensure_vineflower() -> Result<PathBuf, BridgeError> {
    let config = environment_config();
    let dir = resolve_environment_dir(config.vineflower_dir, "vineflower")?;
    crate::deps::ensure_vineflower_in_dir(&dir, &config.vineflower_version)
}

/// 准备打开项目后的基础外部资源。
pub fn ensure_project_resources() -> Result<(), BridgeError> {
    crate::process::find_java()?;
    ensure_vineflower()?;
    Ok(())
}

/// 定位并按需下载 Kotlin 编译依赖。
pub fn ensure_kotlin_dependencies() -> Result<KotlinDependencies, BridgeError> {
    let config = environment_config();
    let dir = resolve_environment_dir(config.kotlin_dependencies_dir, "kotlin")?;
    crate::deps::ensure_kotlin_dependencies_in_dir(&dir, &config.kotlin_version)
}

fn normalize_config(mut config: EnvironmentConfig) -> EnvironmentConfig {
    config.vineflower_version =
        normalize_version(&config.vineflower_version, DEFAULT_VINEFLOWER_VERSION);
    config.kotlin_version = normalize_version(&config.kotlin_version, DEFAULT_KOTLIN_VERSION);
    config.vineflower_dir = normalize_dir(config.vineflower_dir);
    config.kotlin_dependencies_dir = normalize_dir(config.kotlin_dependencies_dir);
    config
}

fn normalize_version(value: &str, default: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn normalize_dir(path: Option<PathBuf>) -> Option<PathBuf> {
    path.filter(|p| !p.as_os_str().is_empty())
}

fn resolve_environment_dir(path: Option<PathBuf>, sub_dir: &str) -> Result<PathBuf, BridgeError> {
    if let Some(path) = path {
        return Ok(path);
    }
    Ok(default_environment_root()?.join(sub_dir))
}

fn default_environment_root() -> Result<PathBuf, BridgeError> {
    let cache_root = decompiler::current_cache_root()?;
    Ok(dependencies_root_from_cache_root(&cache_root))
}

fn dependencies_root_from_cache_root(cache_root: &Path) -> PathBuf {
    cache_root
        .parent()
        .map(|parent| parent.join("dependencies"))
        .unwrap_or_else(|| cache_root.join("dependencies"))
}
