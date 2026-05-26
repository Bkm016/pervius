//! 工作区状态机：用类型约束合法状态组合
//!
//! @author sky

use crate::app::navigate::ClassResolver;
use crate::task::Task;
use crate::ui::search::index::SearchIndex;
use pervius_java_bridge::decompiler::DecompileTask;
use pervius_java_bridge::error::BridgeError;
use pervius_java_bridge::jar::{JarArchive, LoadProgress};
use std::collections::HashSet;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

/// JAR 工作区生命周期状态
pub(crate) enum Workspace {
    /// 未打开任何文件
    Empty,
    /// JAR 正在后台加载
    Loading(LoadingState),
    /// JAR 已加载
    Loaded(LoadedState),
}

/// JAR 后台加载状态
pub(crate) struct LoadingState {
    /// 文件名（用于进度显示）
    pub name: String,
    /// 加载进度（原子量，后台线程写入）
    pub progress: Arc<LoadProgress>,
    /// 后台加载任务
    pub task: Task<Result<JarArchive, BridgeError>>,
    /// 已完成的 JAR 加载结果，等待资源准备完成后再继续。
    pub jar_result: Option<Result<JarArchive, BridgeError>>,
    /// 项目基础资源准备任务（Java / Vineflower 等）。
    pub resource_task: Task<Result<(), BridgeError>>,
    /// 基础资源是否已准备完成。
    pub resources_ready: bool,
}

/// JAR 已加载后的工作状态
pub(crate) struct LoadedState {
    /// JAR 归档
    pub jar: JarArchive,
    /// 反编译阶段
    pub decompile: DecompilePhase,
    /// 后台重反编译启动任务（清缓存 + start 在子线程）
    pub pending_re_decompile: Option<(String, Task<Result<DecompileTask, BridgeError>>)>,
    /// 搜索索引（全量反编译完成后构建）
    pub search_index: Option<Arc<SearchIndex>>,
    /// 搜索索引构建任务
    pub search_index_task: Option<Task<SearchIndex>>,
    /// 搜索索引构建进度（原子计数器，后台线程递增）
    pub search_index_progress: Option<Arc<AtomicU32>>,
    /// 搜索索引构建总数（class 文件数）
    pub search_index_total: u32,
    /// 类名解析器（Shift+Click 导航用）
    pub class_resolver: ClassResolver,
    /// 当前打开项目的临时编译 classpath 条目。
    pub compile_classpath_entries: Vec<std::path::PathBuf>,
}

/// 反编译生命周期阶段
pub(crate) enum DecompilePhase {
    /// 等待用户确认（大文件）或尚未启动
    Pending,
    /// 全量反编译中
    Running {
        /// Vineflower 进程任务（Drop 时自动 kill 子进程）
        task: DecompileTask,
        /// 已反编译的类集合（增量更新）
        completed: HashSet<String>,
    },
    /// 全量反编译完成（或缓存命中）
    Done,
}

impl Workspace {
    /// 获取 JAR 引用（仅 Loaded 状态可用）
    pub fn jar(&self) -> Option<&JarArchive> {
        match self {
            Self::Loaded(s) => Some(&s.jar),
            _ => None,
        }
    }

    /// 获取 JAR 可变引用（仅 Loaded 状态可用）
    pub fn jar_mut(&mut self) -> Option<&mut JarArchive> {
        match self {
            Self::Loaded(s) => Some(&mut s.jar),
            _ => None,
        }
    }

    /// 获取已加载状态引用
    pub fn loaded(&self) -> Option<&LoadedState> {
        match self {
            Self::Loaded(s) => Some(s),
            _ => None,
        }
    }

    /// 获取已加载状态可变引用
    pub fn loaded_mut(&mut self) -> Option<&mut LoadedState> {
        match self {
            Self::Loaded(s) => Some(s),
            _ => None,
        }
    }

    #[allow(dead_code)]
    /// 获取加载中状态引用
    pub fn loading(&self) -> Option<&LoadingState> {
        match self {
            Self::Loading(s) => Some(s),
            _ => None,
        }
    }

    /// 是否有 JAR 处于已加载状态
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    /// 是否正在进行全量反编译
    pub fn is_decompiling(&self) -> bool {
        matches!(self, Self::Loaded(s) if matches!(s.decompile, DecompilePhase::Running { .. }))
    }
}

impl LoadedState {
    /// 创建新的已加载状态
    pub fn new(jar: JarArchive, decompile: DecompilePhase) -> Self {
        let paths = jar.paths();
        let class_resolver = ClassResolver::build(&paths);
        Self {
            jar,
            decompile,
            pending_re_decompile: None,
            search_index: None,
            search_index_task: None,
            search_index_progress: None,
            search_index_total: 0,
            class_resolver,
            compile_classpath_entries: Vec::new(),
        }
    }
}

impl DecompilePhase {
    /// 返回已反编译的类集合（None = 全部已反编译）
    ///
    /// explorer 用此值决定是否显示反编译完成标记：
    /// - `None` → 全部已完成
    /// - `Some(set)` → 仅 set 内的类已完成
    pub fn decompiled_set(&self) -> Option<&HashSet<String>> {
        static EMPTY: std::sync::LazyLock<HashSet<String>> = std::sync::LazyLock::new(HashSet::new);
        match self {
            Self::Done => None,
            Self::Running { completed, .. } => Some(completed),
            Self::Pending => Some(&EMPTY),
        }
    }
}
