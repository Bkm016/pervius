//! Vineflower 反编译器集成：Java 检测、反编译调用、磁盘缓存
//!
//! @author sky

use crate::error::BridgeError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crate::process;

/// 用户设置的反编译缓存根目录（优先于系统 cache_dir）
static CUSTOM_CACHE_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);
/// 当前 Kotlin 类反编译输出模式。
static CURRENT_KOTLIN_MODE: Mutex<KotlinDecompilerMode> = Mutex::new(KotlinDecompilerMode::Vineflower);

/// 缓存完成标记文件名
const CACHE_COMPLETE_MARKER: &str = ".complete";
/// 缓存元数据文件名
const CACHE_META_FILE: &str = ".pervius-cache.toml";

/// 反编译进度（跨线程共享）
pub struct DecompileProgress {
    /// Decompiling 阶段已处理类数
    pub current: AtomicU32,
    /// Preprocessing 阶段统计的根类总数（动态累加）
    pub total: AtomicU32,
    /// 已完成反编译的类路径及其文件夹前缀
    pub decompiled: Mutex<HashSet<String>>,
}

/// Kotlin 类的反编译输出模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KotlinDecompilerMode {
    /// 启用 Vineflower Kotlin 输出，Kotlin 类会生成 `.kt`。
    #[default]
    #[serde(rename = "vineflower")]
    Vineflower,
    /// 关闭 Kotlin 输出，统一回退到 Java 反编译结果。
    #[serde(rename = "java")]
    Java,
}

impl KotlinDecompilerMode {
    fn cache_dir_name(self) -> &'static str {
        match self {
            Self::Vineflower => "kotlin",
            Self::Java => "java",
        }
    }

    fn vineflower_args(self, cmd: &mut process::JavaCommand) {
        if matches!(self, Self::Java) {
            cmd.arg("--kt-enable=0");
        }
    }
}

/// 反编译得到的源码语言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecompiledSourceLanguage {
    Java,
    Kotlin,
}

impl DecompiledSourceLanguage {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            ".java" => Some(Self::Java),
            ".kt" => Some(Self::Kotlin),
            _ => None,
        }
    }
}

/// 后台反编译任务句柄
///
/// Drop 时自动终止正在运行的 vineflower 子进程。
pub struct DecompileTask {
    pub jar_name: String,
    pub progress: Arc<DecompileProgress>,
    pub receiver: mpsc::Receiver<Result<(), BridgeError>>,
    /// 正在运行的子进程 PID（0 表示未运行或已结束）
    child_pid: Arc<AtomicU32>,
}

/// 缓存目录元数据
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CacheMeta {
    /// JAR 文件名
    pub jar_name: String,
    /// JAR 绝对路径
    pub jar_path: String,
    /// 完整哈希
    pub hash: String,
    /// Kotlin 类反编译输出模式
    pub kotlin_mode: Option<KotlinDecompilerMode>,
    /// 已缓存的目录总大小（字节）
    pub size_bytes: Option<u64>,
}

/// 缓存列表条目
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// JAR 文件名
    pub jar_name: String,
    /// JAR 绝对路径
    pub jar_path: String,
    /// 完整哈希
    pub hash: String,
    /// Kotlin 类反编译输出模式
    pub kotlin_mode: KotlinDecompilerMode,
    /// 缓存目录路径
    pub dir: PathBuf,
    /// 最后修改时间（unix epoch 秒）
    pub modified_at: u64,
    /// 目录总大小（字节），旧缓存未知时为 None
    pub size_bytes: Option<u64>,
    /// 是否为完整缓存
    pub complete: bool,
}

impl Drop for DecompileTask {
    fn drop(&mut self) {
        let pid = self.child_pid.load(Ordering::Relaxed);
        if pid == 0 {
            return;
        }
        log::info!("Killing vineflower process (PID {pid})");
        process::kill_tree(pid);
    }
}

/// 设置缓存根目录（空值表示回退到系统默认目录）
pub fn set_cache_root(path: Option<&Path>) {
    let mut lock = CUSTOM_CACHE_ROOT.lock().unwrap_or_else(|p| p.into_inner());
    *lock = path.map(Path::to_path_buf);
}

/// 获取当前生效的缓存根目录
pub fn current_cache_root() -> Result<PathBuf, BridgeError> {
    let custom = CUSTOM_CACHE_ROOT
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let Some(path) = custom {
        return Ok(path);
    }
    let base = dirs::cache_dir().ok_or(BridgeError::NoCacheDir)?;
    Ok(base.join("pervius").join("decompiled"))
}

/// 设置 Kotlin 类反编译输出模式。
pub fn set_kotlin_decompiler_mode(mode: KotlinDecompilerMode) {
    let mut lock = CURRENT_KOTLIN_MODE.lock().unwrap_or_else(|p| p.into_inner());
    *lock = mode;
}

/// 获取当前 Kotlin 类反编译输出模式。
pub fn current_kotlin_decompiler_mode() -> KotlinDecompilerMode {
    *CURRENT_KOTLIN_MODE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 获取缓存根目录
fn cache_root() -> Result<PathBuf, BridgeError> {
    current_cache_root()
}

fn mode_cache_root(mode: KotlinDecompilerMode) -> Result<PathBuf, BridgeError> {
    Ok(cache_root()?.join(mode.cache_dir_name()))
}

/// 获取指定 JAR 哈希的缓存目录
pub fn cache_dir(hash: &str) -> Result<PathBuf, BridgeError> {
    Ok(mode_cache_root(current_kotlin_decompiler_mode())?.join(hash_prefix(hash)))
}

/// 检查缓存是否完整（存在 .complete 标记文件）
pub fn is_cached(hash: &str) -> bool {
    cache_dir(hash)
        .ok()
        .map(|d| d.join(CACHE_COMPLETE_MARKER).exists())
        .unwrap_or(false)
}

/// 清除指定 JAR 的反编译缓存
pub fn clear_cache(hash: &str) {
    if let Ok(dir) = cache_dir(hash) {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 清除指定缓存目录。
pub fn clear_cache_entry_dir(dir: &Path) -> bool {
    std::fs::remove_dir_all(dir).is_ok()
}

/// 清除全部反编译缓存
pub fn clear_all_cache() {
    let Ok(entries) = list_cache_entries() else {
        return;
    };
    for entry in entries {
        let _ = std::fs::remove_dir_all(entry.dir);
    }
}

/// 枚举当前缓存目录下的所有缓存
pub fn list_cache_entries() -> Result<Vec<CacheEntry>, BridgeError> {
    let root = cache_root()?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for mode in [KotlinDecompilerMode::Vineflower, KotlinDecompilerMode::Java] {
        let mode_root = root.join(mode.cache_dir_name());
        if mode_root.exists() {
            collect_cache_entries_from_root(&mut entries, &mode_root, mode)?;
        }
    }
    entries.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.jar_name.cmp(&b.jar_name))
    });
    Ok(entries)
}

fn collect_cache_entries_from_root(
    entries: &mut Vec<CacheEntry>,
    root: &Path,
    fallback_mode: KotlinDecompilerMode,
) -> Result<(), BridgeError> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meta = read_cache_meta(&dir).unwrap_or_default();
        let complete = dir.join(CACHE_COMPLETE_MARKER).exists();
        let hash = if meta.hash.is_empty() {
            dir.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string()
        } else {
            meta.hash.clone()
        };
        let jar_name = if meta.jar_name.is_empty() {
            hash_prefix(&hash).to_string()
        } else {
            meta.jar_name.clone()
        };
        entries.push(CacheEntry {
            jar_name,
            jar_path: meta.jar_path,
            hash,
            kotlin_mode: meta.kotlin_mode.unwrap_or(fallback_mode),
            dir: dir.clone(),
            modified_at: file_modified_at(&dir),
            size_bytes: meta.size_bytes,
            complete,
        });
    }
    Ok(())
}

/// 根据完整 hash 删除指定缓存目录
pub fn clear_cache_entry(hash: &str) -> bool {
    let Ok(entries) = list_cache_entries() else {
        return false;
    };
    let Some(entry) = entries.into_iter().find(|entry| {
        entry.hash == hash || entry.dir.file_name().and_then(|name| name.to_str()) == Some(hash)
    }) else {
        return false;
    };
    clear_cache_entry_dir(&entry.dir)
}

/// 缓存查找结果
#[derive(Clone)]
pub struct CachedSource {
    /// 反编译后的源码（已清理行号标记）
    pub source: String,
    /// 当前反编译结果的源码语言
    pub language: DecompiledSourceLanguage,
    /// 反编译行 → 原始源码行号（1-indexed），None 表示无映射
    pub line_mapping: Vec<Option<u32>>,
}

/// 获取缓存源码文件路径（不读取内容）
pub fn cached_source_path(hash: &str, class_path: &str) -> Option<PathBuf> {
    let dir = cache_dir(hash).ok()?;
    let base = class_to_base_path(class_path);
    for ext in preferred_source_extensions() {
        let file = dir.join(format!("{base}{ext}"));
        log::debug!("cached_source_path: trying {}", file.display());
        if file.exists() {
            return Some(file);
        }
    }
    None
}

/// 从缓存读取反编译源码
///
/// class_path 形如 `com/example/Foo.class` 或 `com/example/Foo$Bar.class`，
/// 自动映射到外层类的 .java 或 .kt 文件（内部类源码在外层类文件中）。
pub fn cached_source(hash: &str, class_path: &str) -> Option<CachedSource> {
    let dir = cache_dir(hash).ok()?;
    let base = class_to_base_path(class_path);
    let result = try_read_source(&dir, base);
    if result.is_none() {
        log::warn!("Cache miss: {class_path} (tried .java and .kt)");
    }
    result
}

/// 从 Vineflower 输出中提取行号标记并移除
///
/// Vineflower 在 `--bytecode-source-mapping=1 --__dump_original_lines__=1` 模式下
/// 会在行尾追加 `// <line_number> [<line_number> ...]` 注释。
/// 返回 (清理后的源码, 每行对应的原始行号映射)。
fn strip_line_markers(raw: &str) -> (String, Vec<Option<u32>>) {
    let mut cleaned = String::with_capacity(raw.len());
    let mut mapping = Vec::new();
    for line in raw.lines() {
        let (text, line_no) = extract_line_marker(line);
        cleaned.push_str(text);
        cleaned.push('\n');
        mapping.push(line_no);
    }
    // 移除末尾多余换行
    if cleaned.ends_with('\n') && !raw.ends_with('\n') {
        cleaned.pop();
    }
    (cleaned, mapping)
}

/// 提取行尾的 Vineflower 行号标记
///
/// 匹配行尾 `// <digits>[ <digits>...]` 格式，取第一个数字作为原始行号。
/// 需要排除正常注释，仅匹配纯数字注释。
fn extract_line_marker(line: &str) -> (&str, Option<u32>) {
    // 从行尾向前查找最后一个 "//"
    let Some(pos) = line.rfind("//") else {
        return (line, None);
    };
    let after = line[pos + 2..].trim();
    // 必须是纯数字（空格分隔），不包含其他字符
    if after.is_empty() {
        return (line, None);
    }
    let is_line_marker = after
        .split_whitespace()
        .all(|tok| tok.parse::<u32>().is_ok());
    if !is_line_marker {
        return (line, None);
    }
    // 取第一个数字作为行号
    let first = after.split_whitespace().next().unwrap();
    let line_no: u32 = first.parse().unwrap();
    // 移除 "//" 和之前的尾部空格
    let text = line[..pos].trim_end();
    (text, Some(line_no))
}

/// .class 条目路径 → 去掉扩展名和内部类后缀的基础路径
///
/// `com/example/Foo$Bar$1.class` → `com/example/Foo`
fn class_to_base_path(class_path: &str) -> &str {
    let without_ext = class_path.strip_suffix(".class").unwrap_or(class_path);
    match without_ext.find('$') {
        Some(pos) => &without_ext[..pos],
        None => without_ext,
    }
}

fn preferred_source_extensions() -> &'static [&'static str] {
    match current_kotlin_decompiler_mode() {
        KotlinDecompilerMode::Vineflower => &[".kt", ".java"],
        KotlinDecompilerMode::Java => &[".java", ".kt"],
    }
}

/// 在目录中尝试读取 .java 或 .kt 反编译源码
fn try_read_source(dir: &Path, base: &str) -> Option<CachedSource> {
    for ext in preferred_source_extensions() {
        let file = dir.join(format!("{base}{ext}"));
        if let Ok(raw) = std::fs::read_to_string(&file) {
            log::debug!("Source found: {}", file.display());
            let (source, line_mapping) = strip_line_markers(&raw);
            return Some(CachedSource {
                source,
                language: DecompiledSourceLanguage::from_extension(ext)?,
                line_mapping,
            });
        }
    }
    None
}

/// 递归统计缓存目录大小
fn dir_size_bytes(dir: &Path) -> Result<u64, BridgeError> {
    let mut size_bytes = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            size_bytes += meta.len();
        }
    }
    Ok(size_bytes)
}

/// 读取缓存元数据
fn read_cache_meta(dir: &Path) -> Option<CacheMeta> {
    let path = dir.join(CACHE_META_FILE);
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// 写入缓存元数据
fn write_cache_meta(output_dir: &Path, jar_path: &Path, jar_name: &str, hash: &str) {
    let meta = CacheMeta {
        jar_name: jar_name.to_string(),
        jar_path: jar_path.to_string_lossy().into_owned(),
        hash: hash.to_string(),
        kotlin_mode: Some(current_kotlin_decompiler_mode()),
        size_bytes: None,
    };
    write_cache_meta_file(output_dir, &meta);
}

/// 更新缓存目录的已统计大小
fn write_cache_size(dir: &Path, size_bytes: u64) {
    let mut meta = read_cache_meta(dir).unwrap_or_default();
    meta.size_bytes = Some(size_bytes);
    write_cache_meta_file(dir, &meta);
}

/// 刷新缓存目录大小并写回 metadata
fn update_cache_size(dir: &Path) {
    let Ok(size_bytes) = dir_size_bytes(dir) else {
        return;
    };
    write_cache_size(dir, size_bytes);
}

/// 写入缓存元数据文件
fn write_cache_meta_file(output_dir: &Path, meta: &CacheMeta) {
    let Ok(content) = toml::to_string(&meta) else {
        return;
    };
    let _ = std::fs::write(output_dir.join(CACHE_META_FILE), content);
}

/// 文件修改时间转 unix epoch 秒
fn file_modified_at(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 显示用 hash 前缀
fn hash_prefix(hash: &str) -> &str {
    &hash[..16.min(hash.len())]
}

/// 将子进程管道转发到 channel（在独立线程中运行）
fn pipe_to_channel(
    stream: impl std::io::Read + Send + 'static,
    tx: mpsc::Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stream);
        for line in reader.lines().flatten() {
            if tx.send(line).is_err() {
                break;
            }
        }
    })
}

/// 启动后台反编译任务
///
/// 在后台线程中运行 `java -jar vineflower.jar <jar_path> <cache_dir>`，
/// 逐行解析 stdout 更新进度，完成后通过 channel 发送结果。
pub fn start(
    jar_path: &Path,
    jar_name: &str,
    hash: &str,
    _class_count: u32,
) -> Result<DecompileTask, BridgeError> {
    process::find_java()?;
    let vineflower = crate::environment::ensure_vineflower()?;
    let output_dir = cache_dir(hash)?;
    std::fs::create_dir_all(&output_dir)?;
    write_cache_meta(&output_dir, jar_path, jar_name, hash);
    let progress = Arc::new(DecompileProgress {
        current: AtomicU32::new(0),
        total: AtomicU32::new(0),
        decompiled: Mutex::new(HashSet::new()),
    });
    let child_pid = Arc::new(AtomicU32::new(0));
    let (tx, rx) = mpsc::channel();
    let jar_path = jar_path.to_path_buf();
    let p = progress.clone();
    let cp = child_pid.clone();
    std::thread::spawn(move || {
        let result = run_vineflower(&vineflower, &jar_path, &output_dir, &p, &cp);
        let _ = tx.send(result);
    });
    Ok(DecompileTask {
        jar_name: jar_name.to_string(),
        progress,
        receiver: rx,
        child_pid,
    })
}

/// 执行 vineflower 进程并解析进度
fn apply_vineflower_source_options(cmd: &mut process::JavaCommand) {
    cmd.arg("--bytecode-source-mapping=1")
        .arg("--__dump_original_lines__=1");
    current_kotlin_decompiler_mode().vineflower_args(cmd);
}

fn run_vineflower(
    vineflower: &Path,
    jar_path: &Path,
    output_dir: &Path,
    progress: &DecompileProgress,
    child_pid: &AtomicU32,
) -> Result<(), BridgeError> {
    let thr = std::thread::available_parallelism()
        .map(|n| n.get().max(2) / 2)
        .unwrap_or(2);
    let mut cmd = process::JavaCommand::new(vineflower)?;
    apply_vineflower_source_options(&mut cmd);
    cmd.arg(format!("-thr={thr}"))
        .arg(jar_path)
        .arg(output_dir);
    let mut child = cmd.spawn().map_err(BridgeError::SpawnFailed)?;
    child_pid.store(child.id(), Ordering::Relaxed);
    // stdout 和 stderr 各起一个线程读取，汇入同一 channel 统一解析
    // Vineflower 可能把进度日志写到任一流，合并后不遗漏且避免管道满死锁
    let (line_tx, line_rx) = mpsc::channel::<String>();
    let stdout_thread = child
        .stdout
        .take()
        .map(|out| pipe_to_channel(out, line_tx.clone()));
    let stderr_thread = child
        .stderr
        .take()
        .map(|err| pipe_to_channel(err, line_tx.clone()));
    drop(line_tx);
    // 进度阶段：
    //   "Preprocessing class" — 分析阶段（每个根类一次）
    //   "Decompiling class"   — 反编译输出阶段（每个根类一次）
    // 两阶段都计入 current，total 在 Preprocessing 时动态累加（×2 留给 Decompiling）
    // "Decompiling class X" 出现时，上一个类已完成输出
    let mut prev_class: Option<String> = None;
    for line in line_rx {
        if line.contains("Preprocessing class") {
            progress.total.fetch_add(2, Ordering::Relaxed);
            progress.current.fetch_add(1, Ordering::Relaxed);
        } else if line.contains("Decompiling class") {
            progress.current.fetch_add(1, Ordering::Relaxed);
            if let Some(prev) = prev_class.take() {
                mark_decompiled(&progress.decompiled, &prev);
            }
            prev_class = extract_class_name(&line);
        } else if line.starts_with("ERROR:") {
            log::error!("vineflower: {line}");
        }
    }
    // 最后一个类
    if let Some(prev) = prev_class {
        mark_decompiled(&progress.decompiled, &prev);
    }
    for t in [stdout_thread, stderr_thread].into_iter().flatten() {
        let _ = t.join();
    }
    let status = child.wait()?;
    child_pid.store(0, Ordering::Relaxed);
    if !status.success() {
        return Err(BridgeError::ExitCode(status.code()));
    }
    // 写入完成标记
    let _ = std::fs::write(output_dir.join(CACHE_COMPLETE_MARKER), "");
    update_cache_size(output_dir);
    Ok(())
}

/// 从 Vineflower 日志行提取类名，统一为 `/` 分隔格式
///
/// Vineflower 日志可能输出 `com.example.Foo`（点）或 `com/example/Foo`（斜杠），
/// 统一转为斜杠格式以匹配树节点路径。
fn extract_class_name(line: &str) -> Option<String> {
    let marker = "Decompiling class ";
    let pos = line.find(marker)?;
    let name = line[pos + marker.len()..].trim();
    tabookit::ensure!(!name.is_empty());
    if name.contains('/') {
        Some(name.to_string())
    } else {
        Some(name.replace('.', "/"))
    }
}

/// 将类名及其文件夹前缀加入已反编译集合
fn mark_decompiled(set: &Mutex<HashSet<String>>, class_name: &str) {
    let mut guard = set.lock().unwrap();
    guard.insert(class_name.to_string());
    for (i, _) in class_name.match_indices('/') {
        guard.insert(class_name[..i + 1].to_string());
    }
}

/// 临时目录 RAII 守卫，Drop 时自动清理
struct TempDirGuard {
    /// 目录路径
    path: PathBuf,
    /// 是否在 Drop 时删除
    should_clean: bool,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.should_clean {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// 单文件反编译：将 class 字节写入临时目录，跑 Vineflower
///
/// `class_path` 形如 `com/example/Foo.class`，用于构建临时目录结构。
/// `jar_path` 用作 Vineflower 的 context library（-e 参数），提供依赖解析；独立文件传 `None` 跳过。
/// `cache_hash` 传入 `Some(hash)` 时输出写入缓存目录（首次预览），传入 `None` 时输出写入临时目录（保存后重编译）。
pub fn decompile_single_class(
    bytes: &[u8],
    class_path: &str,
    jar_path: Option<&Path>,
    jar_name: Option<&str>,
    cache_hash: Option<&str>,
) -> Result<CachedSource, BridgeError> {
    let vineflower = crate::environment::ensure_vineflower()?;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_input = TempDirGuard {
        path: std::env::temp_dir().join(format!("pervius_single_{id}_in")),
        should_clean: true,
    };
    // 有 hash 则写入缓存目录，无则用临时目录
    let output_guard = match cache_hash {
        Some(h) => TempDirGuard {
            path: cache_dir(h)?,
            should_clean: false,
        },
        None => TempDirGuard {
            path: std::env::temp_dir().join(format!("pervius_single_{id}_out")),
            should_clean: true,
        },
    };
    // 独立文件的绝对路径需要取文件名，否则 Path::join 会替换基路径
    let effective_path = if Path::new(class_path).is_absolute() {
        Path::new(class_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(class_path)
    } else {
        class_path
    };
    // 写入临时 .class 文件（保持包目录结构）
    let class_file = tmp_input.path.join(effective_path);
    if let Some(parent) = class_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&output_guard.path)?;
    if let Some(h) = cache_hash {
        write_cache_meta(
            &output_guard.path,
            jar_path.unwrap_or_else(|| Path::new("")),
            jar_name.unwrap_or(class_path),
            h,
        );
    }
    std::fs::write(&class_file, bytes)?;
    let mut cmd = process::JavaCommand::new(&vineflower)?;
    apply_vineflower_source_options(&mut cmd);
    if let Some(jp) = jar_path {
        cmd.arg(format!("-e={}", jp.display()));
    }
    cmd.arg(tmp_input.path.as_os_str())
        .arg(output_guard.path.as_os_str());
    let output = cmd.output().map_err(BridgeError::SpawnFailed)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BridgeError::ProcessFailed(stderr.into_owned()));
    }
    let base = class_to_base_path(effective_path);
    let cached = try_read_source(&output_guard.path, base).ok_or(BridgeError::NoOutput)?;
    if cache_hash.is_some() {
        update_cache_size(&output_guard.path);
    }
    Ok(cached)
}
