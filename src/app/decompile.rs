//! 反编译任务调度与轮询
//!
//! @author sky

use super::workspace::{DecompilePhase, Workspace};
use super::{editor_source_language, App};
use crate::task::{Poll, Pollable, Task};
use crate::ui::editor::view_toggle::ActiveView;
use pervius_java_bridge::decompiler;
use rust_i18n::t;
use std::collections::HashSet;
use std::path::Path;

pub(super) fn spawn_decompile_start_task(
    jar_path: &Path,
    jar_name: &str,
    hash: &str,
    class_count: u32,
) -> (
    String,
    Task<Result<pervius_java_bridge::decompiler::DecompileTask, pervius_java_bridge::error::BridgeError>>,
) {
    let path = jar_path.to_path_buf();
    let name = jar_name.to_string();
    let hash = hash.to_string();
    let thread_name = name.clone();
    let task = Task::spawn(move || decompiler::start(&path, &thread_name, &hash, class_count));
    log::info!("Preparing decompilation: {name} ({class_count} classes)");
    (name, task)
}

impl App {
    /// 轮询 JAR 全量反编译结果
    pub(crate) fn poll_jar_decompile(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        // 更新已反编译类集合
        if let DecompilePhase::Running { task, completed } = &mut loaded.decompile {
            if let Ok(set) = task.progress.decompiled.lock() {
                if !set.is_empty() {
                    *completed = set.clone();
                }
            }
        }
        // 轮询反编译任务完成
        let poll_result = if let DecompilePhase::Running { task, .. } = &loaded.decompile {
            match task.receiver.try_recv() {
                Ok(r) => Some((task.jar_name.clone(), Some(r))),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((task.jar_name.clone(), None))
                }
            }
        } else {
            None
        };
        let Some((jar_name, result)) = poll_result else {
            return;
        };
        loaded.decompile = DecompilePhase::Done;
        match result {
            Some(Ok(())) => {
                log::info!("Decompilation complete: {jar_name}");
                let hash = loaded.jar.hash.as_str();
                self.layout
                    .editor
                    .refresh_class_tabs(Some(hash), &HashSet::new());
                self.toasts
                    .info(t!("layout.decompile_complete", name = jar_name));
                loaded.search_index = None;
            }
            Some(Err(e)) => {
                log::error!("Decompilation failed: {e}");
                self.toasts.error(t!("layout.decompile_failed", error = e));
            }
            None => {
                log::error!("Decompiler thread disconnected: {jar_name}");
            }
        }
    }

    /// 用户确认后启动全量反编译（由 ConfirmAction::DecompileAll 触发）
    pub(crate) fn start_confirmed_decompile(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        if !matches!(loaded.decompile, DecompilePhase::Pending) {
            return;
        }
        if loaded.pending_re_decompile.is_some() {
            return;
        }
        loaded.pending_re_decompile = Some(spawn_decompile_start_task(
            &loaded.jar.path,
            &loaded.jar.name,
            &loaded.jar.hash,
            loaded.jar.class_count(),
        ));
    }

    /// 清除缓存并重新反编译整个 JAR
    pub fn redecompile_jar(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        if matches!(loaded.decompile, DecompilePhase::Running { .. }) {
            self.toasts.warning(t!("layout.decompile_in_progress"));
            return;
        }
        loaded.decompile = DecompilePhase::Pending;
        let path = loaded.jar.path.clone();
        let name = loaded.jar.name.clone();
        let hash = loaded.jar.hash.clone();
        let class_count = loaded.jar.class_count();
        let thread_name = name.clone();
        let task = Task::spawn(move || {
            decompiler::clear_cache(&hash);
            decompiler::start(&path, &thread_name, &hash, class_count)
        });
        log::info!("Re-decompiling: {name} ({class_count} classes)");
        loaded.pending_re_decompile = Some((name, task));
    }

    /// 后台反编译单个 .class 文件
    ///
    /// `write_cache` 为 true 时输出写入缓存目录（首次预览），为 false 时仅返回内存结果。
    /// 有 JAR 上下文时自动传入 `-e` 依赖解析；独立文件无上下文也能反编译。
    pub(super) fn decompile_class(&mut self, entry_path: &str, bytes: Vec<u8>, write_cache: bool) {
        let jar_path = self.workspace.jar().map(|j| j.path.clone());
        let jar_name = self.workspace.jar().map(|j| j.name.clone());
        let hash = if write_cache {
            self.workspace.jar().map(|j| j.hash.clone())
        } else {
            None
        };
        let class_path = entry_path.to_string();
        let cp = class_path.clone();
        let task = Task::spawn(move || {
            decompiler::decompile_single_class(
                &bytes,
                &cp,
                jar_path.as_deref(),
                jar_name.as_deref(),
                hash.as_deref(),
            )
        });
        self.pending_decompiles.push((class_path, task));
    }

    /// 轮询重反编译启动结果
    pub(crate) fn poll_redecompile(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        let Some((_, task)) = &loaded.pending_re_decompile else {
            return;
        };
        let result = match task.poll() {
            Poll::Ready(r) => r,
            Poll::Pending => return,
            Poll::Lost => {
                loaded.pending_re_decompile = None;
                return;
            }
        };
        loaded.pending_re_decompile = None;
        match result {
            Ok(task) => {
                loaded.decompile = DecompilePhase::Running {
                    task,
                    completed: HashSet::new(),
                };
            }
            Err(e) => {
                log::error!("Re-decompile failed to start: {e}");
                self.toasts
                    .warning(t!("layout.decompiler_unavailable", error = e));
            }
        }
    }

    /// 轮询单文件反编译结果，完成后回填到对应 tab
    pub(crate) fn poll_class_decompiles(&mut self) {
        if self.pending_decompiles.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.pending_decompiles.len() {
            let result = match self.pending_decompiles[i].1.poll() {
                Poll::Ready(r) => r,
                Poll::Pending => {
                    i += 1;
                    continue;
                }
                Poll::Lost => {
                    self.pending_decompiles.swap_remove(i);
                    continue;
                }
            };
            let entry_path = self.pending_decompiles.swap_remove(i).0;
            match result {
                Ok(cached) => {
                    let lang = editor_source_language(cached.language);
                    for (_, tab) in self.layout.editor.dock_state.iter_all_tabs_mut() {
                        if tab.entry_path.as_deref() == Some(&entry_path) {
                            tab.set_decompiled(
                                cached.source.clone(),
                                lang,
                                cached.line_mapping.clone(),
                            );
                            if tab.active_view == ActiveView::Hex {
                                tab.active_view = ActiveView::Decompiled;
                            }
                            break;
                        }
                    }
                    // 已修改条目的反编译结果缓存到 JAR 内存（磁盘缓存无效）
                    if let Some(jar) = self.workspace.jar_mut() {
                        if jar.is_modified(&entry_path) {
                            jar.put_decompiled(&entry_path, cached);
                        }
                    }
                    if let Some(loaded) = self.workspace.loaded_mut() {
                        if let DecompilePhase::Running { completed, .. } = &mut loaded.decompile {
                            let base = entry_path.strip_suffix(".class").unwrap_or(&entry_path);
                            let base = match base.find('$') {
                                Some(pos) => &base[..pos],
                                None => base,
                            };
                            completed.insert(base.to_string());
                            for (idx, _) in base.match_indices('/') {
                                completed.insert(base[..idx + 1].to_string());
                            }
                        }
                    }
                    log::info!("Single-decompiled: {entry_path}");
                }
                Err(e) => {
                    log::warn!("Single decompile failed: {entry_path}: {e}");
                    self.toasts.error(t!("layout.decompile_failed", error = e));
                }
            }
        }
    }
}
