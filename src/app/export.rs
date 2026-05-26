//! 导出功能：反编译源码导出、JAR 导出
//!
//! @author sky

use super::App;
use crate::task::{Poll, Pollable, Task};
use pervius_java_bridge::decompiler;
use pervius_java_bridge::jar::LoadProgress;
use rust_i18n::t;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// JAR 写出模式
pub(crate) enum JarWriteMode {
    /// 另存为新的 JAR 文件
    ExportCopy,
    /// 覆盖当前打开的源 JAR 文件
    OverwriteSource,
}

/// JAR 后台导出状态
pub(crate) struct ExportingState {
    /// 目标文件路径（用于完成后显示）
    pub dest: PathBuf,
    /// 已修改条目数（用于完成后显示）
    pub modified_count: usize,
    /// 写出模式
    pub mode: JarWriteMode,
    /// 导出进度
    pub progress: Arc<LoadProgress>,
    /// 后台导出任务
    pub task: Task<Result<usize, String>>,
}

impl App {
    fn start_jar_write(
        &mut self,
        dest: PathBuf,
        modified_count: usize,
        snapshot: Vec<(String, Vec<u8>)>,
        mode: JarWriteMode,
    ) {
        let progress = Arc::new(LoadProgress::new());
        let p = progress.clone();
        let out = dest.clone();
        let task = Task::spawn(move || {
            pervius_java_bridge::jar::write_jar(&snapshot, &out, &p).map_err(|e| e.to_string())
        });
        self.exporting = Some(ExportingState {
            dest,
            modified_count,
            mode,
            progress,
            task,
        });
    }

    /// 导出修改后的 JAR 到用户选择的路径
    ///
    /// 在主线程快照条目数据，然后在后台线程写出 ZIP，
    /// 通过 `LoadProgress` 原子量回报进度。
    pub fn export_jar(&mut self) {
        let Some(jar) = self.workspace.jar() else {
            self.toasts.warning(t!("layout.export_no_jar"));
            return;
        };
        if self.exporting.is_some() {
            self.toasts.warning(t!("layout.export_in_progress"));
            return;
        }
        let unsaved = self.layout.editor.unsaved_paths();
        if !unsaved.is_empty() {
            self.toasts
                .warning(t!("layout.export_unsaved", count = unsaved.len()));
            return;
        }
        let default_name = jar
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output.jar".to_owned());
        let Some(dest) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("JAR", &["jar"])
            .save_file()
        else {
            return;
        };
        let snapshot = jar.snapshot_entries();
        let modified_count = jar.modified_count();
        self.start_jar_write(dest, modified_count, snapshot, JarWriteMode::ExportCopy);
    }

    /// 保存并覆盖当前打开的源 JAR 文件
    pub fn save_jar_overwrite_source(&mut self) {
        let Some(jar) = self.workspace.jar() else {
            self.toasts.warning(t!("layout.export_no_jar"));
            return;
        };
        if self.exporting.is_some() {
            self.toasts.warning(t!("layout.save_source_in_progress"));
            return;
        }
        let unsaved = self.layout.editor.unsaved_paths();
        if !unsaved.is_empty() {
            self.toasts
                .warning(t!("layout.export_unsaved", count = unsaved.len()));
            return;
        }
        let modified_count = jar.modified_count();
        if modified_count == 0 {
            self.toasts.info(t!("layout.save_source_no_changes"));
            return;
        }
        let dest = jar.path.clone();
        let snapshot = jar.snapshot_entries();
        self.start_jar_write(dest, modified_count, snapshot, JarWriteMode::OverwriteSource);
    }

    /// 轮询后台 JAR 导出是否完成
    pub(crate) fn poll_export_jar(&mut self) {
        let Some(state) = &self.exporting else {
            return;
        };
        let result = match state.task.poll() {
            Poll::Ready(r) => r,
            Poll::Pending => return,
            Poll::Lost => {
                self.exporting = None;
                return;
            }
        };
        let state = self.exporting.take().unwrap();
        let dest = state.dest;
        let modified = state.modified_count;
        let mode = state.mode;
        match result {
            Ok(count) => {
                let display = dest.to_string_lossy();
                match mode {
                    JarWriteMode::ExportCopy => {
                        self.toasts.info(t!(
                            "layout.export_jar_complete",
                            path = display,
                            count = count,
                            modified = modified
                        ));
                        log::info!(
                            "Exported JAR ({count} entries, {modified} modified) to {display}"
                        );
                    }
                    JarWriteMode::OverwriteSource => {
                        if let Some(jar) = self.workspace.jar_mut() {
                            if let Err(error) = jar.commit_modified_from_file(&dest) {
                                log::warn!("Failed to refresh saved source JAR metadata: {error}");
                                jar.clear_modified();
                            }
                        }
                        self.toasts.info(t!(
                            "layout.save_source_complete",
                            path = display,
                            count = count,
                            modified = modified
                        ));
                        log::info!(
                            "Saved source JAR ({count} entries, {modified} modified) to {display}"
                        );
                    }
                }
            }
            Err(e) => match mode {
                JarWriteMode::ExportCopy => {
                    self.toasts.error(t!("layout.export_failed", error = e));
                    log::error!("Export JAR failed: {e}");
                }
                JarWriteMode::OverwriteSource => {
                    self.toasts
                        .error(t!("layout.save_source_failed", error = e));
                    log::error!("Save source JAR failed: {e}");
                }
            },
        }
    }

    /// 导出反编译源码到用户选择的目录
    ///
    /// 将 Vineflower 缓存目录中的 `.java` / `.kt` 文件复制到目标目录，
    /// 保持原始包结构。
    pub fn export_decompiled(&mut self) {
        let Some(jar) = self.workspace.jar() else {
            self.toasts.warning(t!("layout.export_no_jar"));
            return;
        };
        if self.workspace.is_decompiling() {
            self.toasts.warning(t!("layout.decompile_in_progress"));
            return;
        }
        if !decompiler::is_cached(&jar.hash) {
            self.toasts.warning(t!("layout.export_not_decompiled"));
            return;
        }
        let cache = match decompiler::cache_dir(&jar.hash) {
            Ok(d) if d.exists() => d,
            _ => {
                self.toasts.warning(t!("layout.export_not_decompiled"));
                return;
            }
        };
        let Some(dest) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        match copy_sources(&cache, &dest) {
            Ok(count) => {
                let display = dest.to_string_lossy();
                self.toasts
                    .info(t!("layout.export_complete", path = display, count = count));
                log::info!("Exported {count} files to {display}");
            }
            Err(e) => {
                self.toasts.error(t!("layout.export_failed", error = e));
                log::error!("Export failed: {e}");
            }
        }
    }
}

/// 递归复制 `.java` / `.kt` 源码文件到目标目录，保持目录结构
///
/// 跳过 `.complete` 等非源码文件，返回复制的文件数。
fn copy_sources(src: &Path, dest: &Path) -> Result<usize, String> {
    let mut count = 0;
    copy_sources_recursive(src, src, dest, &mut count)?;
    Ok(count)
}

/// 递归遍历源目录，复制匹配的源码文件
fn copy_sources_recursive(
    root: &Path,
    dir: &Path,
    dest: &Path,
    count: &mut usize,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{e}"))?;
        let path = entry.path();
        if path.is_dir() {
            copy_sources_recursive(root, &path, dest, count)?;
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "java" && ext != "kt" {
                continue;
            }
            let rel = path.strip_prefix(root).map_err(|e| format!("{e}"))?;
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
            }
            std::fs::copy(&path, &target).map_err(|e| format!("{e}"))?;
            *count += 1;
        }
    }
    Ok(())
}
