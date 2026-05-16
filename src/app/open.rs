//! 文件打开：JAR 加载、拖拽处理、explorer 点击、独立文件打开
//!
//! @author sky

use super::decompile::spawn_decompile_start_task;
use super::workspace::{DecompilePhase, LoadedState, LoadingState, Workspace};
use super::App;
use crate::app::ConfirmAction;
use crate::task::{Poll, Pollable, Task};
use crate::ui::editor::EditorArea;
use crate::ui::explorer::tree;
use eframe::egui;
use egui_shell::components::SettingsFile;
use pervius_java_bridge::{decompiler, environment};
use pervius_java_bridge::error::BridgeError;
use pervius_java_bridge::jar::{JarArchive, LoadProgress};
use rust_i18n::t;
use std::path::Path;
use std::sync::Arc;

/// 自动全量反编译的文件大小阈值（1 MB）
const FULL_DECOMPILE_THRESHOLD: u64 = 1_000_000;

type PendingDecompileStart = (String, Task<Result<decompiler::DecompileTask, BridgeError>>);

impl App {
    /// 处理 explorer 中点击的文件
    pub(crate) fn handle_pending_open(&mut self) {
        let Some(entry_path) = self.layout.file_panel.pending_open.take() else {
            return;
        };
        // 已打开的 tab 直接聚焦
        if self.layout.editor.focus_tab(&entry_path) {
            return;
        }
        let Some(jar) = self.workspace.jar() else {
            return;
        };
        let Some(raw) = jar.get(&entry_path) else {
            return;
        };
        let bytes = raw.to_vec();
        let hash = jar.hash.as_str();
        let is_modified = jar.is_modified(&entry_path);
        // 已修改条目优先从 JAR 内存缓存读取反编译结果
        let mem_cached = if is_modified {
            jar.get_decompiled(&entry_path).cloned()
        } else {
            None
        };
        let has_cache = !is_modified && decompiler::cached_source_path(hash, &entry_path).is_some();
        let tab = Self::create_tab(&entry_path, &bytes, Some(hash), mem_cached.as_ref());
        self.layout.editor.open_tab(tab);
        if entry_path.ends_with(".class") {
            if is_modified && mem_cached.is_none() {
                self.decompile_class(&entry_path, bytes, false);
            } else if !is_modified && !has_cache {
                self.decompile_class(&entry_path, bytes, true);
            }
        }
    }

    /// 处理拖拽到窗口的文件
    pub(crate) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        // 多 pass 去重
        let frame = ctx.cumulative_frame_nr();
        let cache_id = egui::Id::new("drop_frame");
        let last: Option<u64> = ctx.data(|d| d.get_temp(cache_id));
        if last == Some(frame) {
            return;
        }
        ctx.data_mut(|d| d.insert_temp(cache_id, frame));
        if let Some(path) = dropped.into_iter().find_map(|file| file.path) {
            if is_jar_file(&path) {
                self.request_open_jar(&path);
            } else {
                self.open_standalone_file(&path);
            }
        }
    }

    /// 检查后台 JAR 加载和基础资源准备是否完成
    pub(crate) fn check_loading(&mut self) {
        let result = {
            let Workspace::Loading(loading) = &mut self.workspace else {
                return;
            };
            poll_loading_result(loading)
        };
        let Some(result) = result else {
            return;
        };
        match result {
            Ok(jar) => self.finish_loading_jar(jar),
            Err(e) => {
                log::error!("Failed to open JAR: {e}");
                self.toasts.error(t!("layout.open_jar_failed", error = e));
                self.workspace = Workspace::Empty;
            }
        }
    }

    fn finish_loading_jar(&mut self, jar: JarArchive) {
        let paths = jar.paths();
        self.layout.file_panel.roots = tree::build_tree(&jar.name, &paths);
        self.layout.file_panel.selected = None;
        self.layout.file_panel.filter.clear();
        let (decompile, pending_start) = initial_decompile_state(&jar, &mut self.pending_confirm);
        self.settings.add_recent(&jar.path, &jar.name);
        if let Err(e) = self.settings.save() {
            log::warn!("保存最近打开记录失败: {e}");
        }
        let mut loaded = LoadedState::new(jar, decompile);
        loaded.pending_re_decompile = pending_start;
        self.workspace = Workspace::Loaded(loaded);
        self.layout.explorer_visible = true;
    }

    /// 从最近打开列表移除指定路径并持久化
    pub fn remove_recent(&mut self, path: &Path) {
        self.settings.remove_recent(path);
        if let Err(e) = self.settings.save() {
            log::warn!("保存最近打开记录失败: {e}");
        }
    }

    /// 选择文件/目录并添加到当前会话 classpath。
    pub fn add_classpath_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter(&*t!("layout.java_archive"), &["jar", "zip", "war", "ear"])
            .pick_files()
        {
            self.add_classpath_paths(paths);
        }
    }

    pub(crate) fn add_classpath_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        let mut added = 0;
        for path in paths {
            if self.add_session_classpath_entry(path) {
                added += 1;
            }
        }
        if added > 0 {
            self.toasts
                .success(t!("layout.classpath_added", count = added));
        }
    }

    /// 在后台线程打开 JAR 文件
    pub fn open_jar(&mut self, path: &Path) {
        // 前置检查：文件不存在时直接提示，不清空旧状态
        if !path.exists() {
            self.toasts
                .error(t!("layout.file_not_found", path = path.display()));
            self.remove_recent(path);
            return;
        }
        let progress = Arc::new(LoadProgress::new());
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let path = path.to_path_buf();
        let p = progress.clone();
        let task = Task::spawn(move || JarArchive::open_with_progress(&path, &p));
        let resource_task = Task::spawn(environment::ensure_project_resources);
        self.layout.file_panel.roots = Vec::new();
        self.layout.file_panel.selected = None;
        self.layout.editor = EditorArea::new();
        self.pending_decompiles.clear();
        self.pending_compiles.clear();
        self.exporting = None;
        self.workspace = Workspace::Loading(LoadingState {
            name,
            progress,
            task,
            jar_result: None,
            resource_task,
            resources_ready: false,
        });
    }

    /// 打开文件选择器，返回 (路径, 是否 JAR 类文件)
    pub(crate) fn pick_file() -> Option<(std::path::PathBuf, bool)> {
        let jar_label = t!("layout.java_archive");
        let class_label = t!("layout.class_file");
        rfd::FileDialog::new()
            .add_filter(&*jar_label, &["jar", "zip", "war", "ear"])
            .add_filter(&*class_label, &["class"])
            .pick_file()
            .map(|path| {
                let is_jar = is_jar_file(&path);
                (path, is_jar)
            })
    }

    /// 启动新进程实例打开指定文件
    pub(crate) fn spawn_new_window(&mut self, path: &Path) {
        match std::env::current_exe() {
            Ok(exe) => {
                if let Err(e) = std::process::Command::new(&exe).arg(path).spawn() {
                    log::error!("Failed to spawn new window: {e}");
                    self.toasts
                        .error(t!("layout.spawn_window_failed", error = e));
                }
            }
            Err(e) => {
                log::error!("Cannot determine executable path: {e}");
                self.toasts
                    .error(t!("layout.spawn_window_failed", error = e));
            }
        }
    }

    /// 打开文件对话框选择 JAR 或独立文件
    pub fn open_jar_dialog(&mut self) {
        if let Some((path, is_jar)) = Self::pick_file() {
            if is_jar {
                self.open_jar(&path);
            } else {
                self.open_standalone_file(&path);
            }
        }
    }

    /// 打开独立文件（非 JAR 条目）为 tab，保存时直接写回磁盘
    pub fn open_standalone_file(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();
        // 已打开则聚焦
        if self.layout.editor.focus_tab(&path_str) {
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to read file: {path_str}: {e}");
                self.toasts.error(t!("layout.open_file_failed", error = e));
                return;
            }
        };
        let mut tab = Self::create_tab(&path_str, &bytes, None, None);
        tab.standalone_path = Some(path.to_path_buf());
        self.layout.editor.open_tab(tab);
        // 独立 .class 文件触发反编译（无 JAR 上下文）
        if path_str.ends_with(".class") {
            self.decompile_class(&path_str, bytes, false);
        }
    }
}

fn poll_loading_result(loading: &mut LoadingState) -> Option<Result<JarArchive, BridgeError>> {
    poll_loading_jar(loading);
    match poll_loading_resources(loading) {
        Some(Ok(())) => loading.jar_result.take(),
        Some(Err(error)) => Some(Err(error)),
        None => None,
    }
}

fn poll_loading_jar(loading: &mut LoadingState) {
    if loading.jar_result.is_some() {
        return;
    }
    loading.jar_result = Some(match loading.task.poll() {
        Poll::Ready(result) => result,
        Poll::Pending => return,
        Poll::Lost => Err(loading_task_lost("JAR loading")),
    });
}

fn poll_loading_resources(loading: &mut LoadingState) -> Option<Result<(), BridgeError>> {
    if loading.resources_ready {
        return Some(Ok(()));
    }
    match loading.resource_task.poll() {
        Poll::Ready(Ok(())) => {
            loading.resources_ready = true;
            Some(Ok(()))
        }
        Poll::Ready(Err(error)) => Some(Err(error)),
        Poll::Pending => None,
        Poll::Lost => Some(Err(loading_task_lost("resource preparation"))),
    }
}

fn loading_task_lost(name: &str) -> BridgeError {
    BridgeError::Download(format!("{name} task disconnected"))
}

/// 判断路径是否为 JAR 类归档文件
fn is_jar_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jar" | "zip" | "war" | "ear"
    )
}

/// 决定 JAR 加载完成后的初始反编译阶段
fn initial_decompile_state(
    jar: &JarArchive,
    pending_confirm: &mut Option<ConfirmAction>,
) -> (DecompilePhase, Option<PendingDecompileStart>) {
    if decompiler::is_cached(&jar.hash) {
        log::info!("Decompiled cache hit for {}", jar.name);
        return (DecompilePhase::Done, None);
    }
    if jar.file_size <= FULL_DECOMPILE_THRESHOLD {
        return (
            DecompilePhase::Pending,
            Some(spawn_decompile_start_task(
                &jar.path,
                &jar.name,
                &jar.hash,
                jar.class_count(),
            )),
        );
    }
    *pending_confirm = Some(ConfirmAction::DecompileAll);
    (DecompilePhase::Pending, None)
}
