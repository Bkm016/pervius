//! Java 源码编译任务调度与轮询
//!
//! @author sky

use super::App;
use crate::task::{Poll, Pollable, Task};
use egui_editor::highlight::Language;
use pervius_java_bridge::compiler::{
    self, CompileClasspath, CompileDiagnostic, CompileOutcome, CompiledClass, DiagSeverity,
    KotlinSource,
};
use pervius_java_bridge::error::BridgeError;
use rust_i18n::t;

pub(crate) struct PendingCompile {
    /// 触发编译的 tab/class 路径。
    pub entry_path: String,
    /// 编译开始时的源码快照，用于避免异步完成后覆盖用户的新编辑。
    pub source_snapshot: String,
    pub task: Task<Result<CompileOutcome, BridgeError>>,
}

struct CompileRequest {
    entry_path: String,
    source: String,
    language: Language,
    target: u8,
    snapshot: String,
}

fn class_info_release(version: Option<&str>) -> Option<u8> {
    let version = version?;
    let java = version.strip_prefix("Java ")?;
    let release = java.split_whitespace().next()?;
    let release = release.strip_prefix("1.").unwrap_or(release);
    release.parse::<u8>().ok()
}

fn kotlin_source_path(entry_path: &str) -> String {
    let base = entry_path.strip_suffix(".class").unwrap_or(entry_path);
    let outer = base.find('$').map_or(base, |idx| &base[..idx]);
    format!("{outer}.kt")
}

impl App {
    /// 构建本次编译使用的 classpath：当前 JAR + 会话条目 + 全局设置条目。
    pub(crate) fn compile_classpath(&self) -> CompileClasspath {
        let mut classpath = CompileClasspath::new();
        if let Some(jar) = self.workspace.jar() {
            classpath.push(jar.path.clone());
        }
        if let Some(loaded) = self.workspace.loaded() {
            for path in &loaded.compile_classpath_entries {
                classpath.push(path.clone());
            }
        }
        for path in &self.settings.compile.classpath_entries {
            classpath.push(path);
        }
        classpath
    }

    /// 向当前会话 classpath 追加条目。
    pub(crate) fn add_session_classpath_entry(&mut self, path: std::path::PathBuf) -> bool {
        let Some(loaded) = self.workspace.loaded_mut() else {
            return false;
        };
        if loaded.compile_classpath_entries.iter().any(|p| p == &path) {
            return false;
        }
        loaded.compile_classpath_entries.push(path);
        true
    }

    /// 启动指定 class tab 的源码编译
    pub(crate) fn compile_source_tab(&mut self, entry_path: &str) {
        let Some(request) = self.take_compile_request(entry_path) else {
            return;
        };
        if !compiler::is_jdk_available() {
            self.toasts.error(t!("editor.jdk_required"));
            return;
        }
        let task = self.spawn_compile_task(&request);
        self.pending_compiles.push(PendingCompile {
            entry_path: request.entry_path,
            source_snapshot: request.snapshot,
            task,
        });
        log::info!("Compiling source: {entry_path}");
    }

    fn take_compile_request(&mut self, entry_path: &str) -> Option<CompileRequest> {
        for (_, tab) in self.layout.editor.dock_state.iter_all_tabs_mut() {
            if tab.entry_path.as_deref() != Some(entry_path) {
                continue;
            }
            if !tab.is_class || !tab.is_source_unlocked() {
                return None;
            }
            if tab.is_modified {
                self.toasts.warning(t!("editor.source_vs_struct_conflict"));
                return None;
            }
            let Some(target) = class_info_release(tab.class_info.as_deref()) else {
                self.toasts.error(t!(
                    "editor.recompile_failed",
                    error = "Cannot determine original class version"
                ));
                return None;
            };
            tab.compile_diagnostics.clear();
            let source = tab.decompiled.clone();
            return Some(CompileRequest {
                entry_path: entry_path.to_string(),
                source: source.clone(),
                language: tab.language,
                target,
                snapshot: source,
            });
        }
        None
    }

    fn spawn_compile_task(
        &self,
        request: &CompileRequest,
    ) -> Task<Result<CompileOutcome, BridgeError>> {
        let classpath = self.compile_classpath();
        let skip_metadata_version_check = self.settings.compile.kotlin_skip_metadata_version_check;
        let entry_path = request.entry_path.clone();
        let binary_name = request
            .entry_path
            .strip_suffix(".class")
            .unwrap_or(&request.entry_path)
            .to_string();
        let source = request.source.clone();
        let language = request.language;
        let target = Some(request.target);
        Task::spawn(move || {
            if language == Language::Kotlin {
                let sources = [KotlinSource {
                    path: kotlin_source_path(&entry_path),
                    source,
                }];
                compiler::compile_kotlin_sources_with_options(
                    &sources,
                    &classpath,
                    target,
                    None,
                    skip_metadata_version_check,
                )
            } else {
                compiler::compile_source(&source, &binary_name, &classpath, target, true)
            }
        })
    }

    /// 轮询 class 源码编译结果，成功后写回 JAR 并刷新反编译源码
    pub(crate) fn poll_class_compiles(&mut self) {
        if self.pending_compiles.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.pending_compiles.len() {
            let result = match self.pending_compiles[i].task.poll() {
                Poll::Ready(r) => r,
                Poll::Pending => {
                    i += 1;
                    continue;
                }
                Poll::Lost => {
                    self.pending_compiles.swap_remove(i);
                    continue;
                }
            };
            let pending = self.pending_compiles.swap_remove(i);
            match result {
                Ok(CompileOutcome::Success(classes)) => {
                    self.handle_compile_success(
                        &pending.entry_path,
                        &pending.source_snapshot,
                        classes,
                    );
                }
                Ok(CompileOutcome::Errors(diagnostics)) => {
                    self.handle_compile_errors(&pending.entry_path, diagnostics);
                }
                Ok(CompileOutcome::JdkMissing) => {
                    self.toasts.error(t!("editor.jdk_required"));
                }
                Err(e) => {
                    log::error!("Compile task failed: {}: {e}", pending.entry_path);
                    self.toasts
                        .error(t!("editor.recompile_failed", error = e.to_string()));
                }
            }
        }
    }

    fn handle_compile_success(
        &mut self,
        entry_path: &str,
        source_snapshot: &str,
        classes: Vec<CompiledClass>,
    ) {
        let source_snapshot_is_current = self
            .layout
            .editor
            .dock_state
            .iter_all_tabs()
            .find(|(_, tab)| tab.entry_path.as_deref() == Some(entry_path))
            .map(|(_, tab)| tab.decompiled == source_snapshot)
            .unwrap_or(true);
        if !source_snapshot_is_current {
            log::warn!("Discarding stale compile result: {entry_path}");
            return;
        }
        let mut main_bytes = None;
        let main_binary = entry_path.trim_end_matches(".class");
        let snapshot = source_snapshot.to_string();
        let count = classes.len();
        for class in classes {
            let class_path = format!("{}.class", class.binary_name);
            if class.binary_name == main_binary {
                main_bytes = Some(class.bytes.clone());
            }
            if let Some(jar) = self.workspace.jar_mut() {
                jar.put(&class_path, class.bytes.clone());
            }
            for (_, tab) in self.layout.editor.dock_state.iter_all_tabs_mut() {
                if tab.entry_path.as_deref() == Some(&class_path) {
                    tab.commit_save(class.bytes.clone());
                    tab.source_modified = false;
                    if class_path == entry_path {
                        tab.decompiled = snapshot.clone();
                        tab.original_source = snapshot.clone();
                        tab.refresh_decompiled_data();
                        tab.compile_diagnostics.clear();
                    }
                }
            }
        }
        if let Some(bytes) = main_bytes {
            self.decompile_class(entry_path, bytes, false);
        }
        self.toasts
            .success(t!("editor.recompile_success", n = count));
        log::info!("Compiled source: {entry_path} ({count} classes)");
    }

    fn handle_compile_errors(&mut self, entry_path: &str, diagnostics: Vec<CompileDiagnostic>) {
        let first = diagnostics
            .iter()
            .find(|d| d.severity == DiagSeverity::Error)
            .or_else(|| diagnostics.first());
        for (_, tab) in self.layout.editor.dock_state.iter_all_tabs_mut() {
            if tab.entry_path.as_deref() == Some(entry_path) {
                tab.compile_diagnostics = diagnostics.clone();
                if let Some(diag) = first {
                    if diag.line > 0 {
                        tab.pending_scroll_to_line = Some(diag.line.saturating_sub(1) as usize);
                    }
                }
                break;
            }
        }
        let message = first
            .map(|d| {
                if d.line > 0 {
                    format!("{}:{} {}", d.line, d.column, d.message)
                } else {
                    d.message.clone()
                }
            })
            .unwrap_or_else(|| "javac reported errors".to_string());
        self.toasts
            .error(t!("editor.recompile_failed", error = message));
        log::warn!("Compile failed: {entry_path}");
    }
}
