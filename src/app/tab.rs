//! Tab 生命周期：创建、保存、编码检测
//!
//! @author sky

use super::{editor_source_language, App};
use crate::ui::editor::view_toggle::ActiveView;
use crate::ui::editor::EditorTab;
use egui_editor::highlight::Language;
use pervius_java_bridge::decompiler::{self, CachedSource};
use rust_i18n::t;

impl App {
    /// 保存当前聚焦 tab 的修改
    ///
    /// 独立文件直接写回磁盘；JAR 条目写入 JAR 内存并触发重反编译。
    pub fn save_active_tab(&mut self) {
        let compile_entry = {
            let Some(tab) = self.layout.editor.focused_tab_mut() else {
                return;
            };
            if !(tab.is_modified || tab.source_modified) {
                return;
            }
            if tab.standalone_path.is_none()
                && tab.is_class
                && tab.is_source_unlocked()
                && tab.source_modified
            {
                tab.entry_path.clone()
            } else {
                None
            }
        };
        if let Some(entry_path) = compile_entry {
            self.compile_source_tab(&entry_path);
            return;
        }
        let Some(tab) = self.layout.editor.focused_tab_mut() else {
            return;
        };
        // 独立 class 源码编辑同样必须走编译通道；只有结构化字节码编辑才会走 apply_structure/patch_methods。
        if tab.standalone_path.is_some()
            && tab.is_class
            && tab.is_source_unlocked()
            && tab.source_modified
        {
            if tab.is_modified {
                self.toasts
                    .warning(t!("editor.source_vs_struct_conflict"));
                return;
            }
            if let Some(entry_path) = tab.entry_path.clone() {
                self.compile_source_tab(&entry_path);
            }
            return;
        }
        // 独立文件：直接写回磁盘，不参与 JAR modified 管理
        if let Some(disk_path) = tab.standalone_path.clone() {
            let new_bytes = match tab.serialize_bytes(None) {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Save failed: {}: {e}", disk_path.display());
                    self.toasts.error(t!("editor.save_failed", error = e));
                    return;
                }
            };
            if let Err(e) = std::fs::write(&disk_path, &new_bytes) {
                log::error!("Write failed: {}: {e}", disk_path.display());
                self.toasts.error(t!("layout.write_file_failed", error = e));
                return;
            }
            tab.commit_save(new_bytes.clone());
            let is_class = tab.is_class;
            let entry_path = tab.entry_path.clone();
            log::info!("Saved standalone: {}", disk_path.display());
            if is_class {
                if let Some(ep) = entry_path {
                    self.decompile_class(&ep, new_bytes, false);
                }
            }
            return;
        }
        let Some(entry_path) = tab.entry_path.clone() else {
            return;
        };
        // 文本文件：序列化后写入 JAR
        if tab.is_text {
            let new_bytes = tab.serialize_bytes(None).unwrap();
            tab.commit_save(new_bytes.clone());
            if let Some(jar) = self.workspace.jar_mut() {
                jar.put(&entry_path, new_bytes);
            }
            log::info!("Saved text: {entry_path}");
            return;
        }
        // class 文件：apply structure → 重建 CS → 写入 JAR → 重反编译
        let jar_path = self.workspace.jar().map(|j| j.path.as_path());
        match tab.serialize_bytes(jar_path) {
            Ok(new_bytes) => {
                let saved_set = tab
                    .class_structure
                    .as_ref()
                    .map(|cs| cs.collect_saved_members())
                    .unwrap_or_default();
                tab.commit_save(new_bytes.clone());
                if let Some(cs) = &mut tab.class_structure {
                    cs.restore_saved_flags(&saved_set);
                }
                self.layout
                    .editor
                    .saved_members
                    .insert(entry_path.clone(), saved_set);
                if let Some(jar) = self.workspace.jar_mut() {
                    jar.put(&entry_path, new_bytes.clone());
                }
                log::info!("Saved: {entry_path}");
                self.decompile_class(&entry_path, new_bytes, false);
            }
            Err(e) => {
                log::error!("Save failed: {entry_path}: {e}");
                self.toasts.error(t!("editor.save_failed", error = e));
            }
        }
    }

    /// 从条目路径和字节创建编辑器 tab
    ///
    /// `mem_cached` 为已修改条目的内存反编译缓存，优先于磁盘缓存。
    pub(super) fn create_tab(
        entry_path: &str,
        bytes: &[u8],
        jar_hash: Option<&str>,
        mem_cached: Option<&CachedSource>,
    ) -> EditorTab {
        let file_name = entry_path.rsplit('/').next().unwrap_or(entry_path);
        let title = file_name.strip_suffix(".class").unwrap_or(file_name);
        if file_name.ends_with(".class") {
            let cached = mem_cached
                .cloned()
                .or_else(|| jar_hash.and_then(|h| decompiler::cached_source(h, entry_path)));
            let lang = cached
                .as_ref()
                .map(|cached| editor_source_language(cached.language))
                .unwrap_or(Language::Java);
            let mut tab = EditorTab::new_class(title, entry_path, bytes.to_vec(), lang);
            if let Some(c) = cached {
                tab.set_decompiled(c.source, lang, c.line_mapping);
                tab.active_view = ActiveView::Decompiled;
            }
            tab
        } else if Self::is_binary(bytes) {
            EditorTab::new_binary(title, entry_path, bytes.to_vec())
        } else {
            let text = Self::decode_text(bytes);
            let lang = Language::from_filename(file_name);
            EditorTab::new_text(title, entry_path, text, bytes.to_vec(), lang)
        }
    }

    /// 判断字节内容是否为二进制文件（前 8KB 内含 null 字节即视为二进制）
    fn is_binary(bytes: &[u8]) -> bool {
        let check_len = bytes.len().min(8192);
        bytes[..check_len].contains(&0)
    }

    /// 将字节解码为文本（自动检测编码）
    fn decode_text(bytes: &[u8]) -> String {
        // UTF-8 快速路径
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        // 非 UTF-8: 用 chardetng 检测编码后转换
        let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
        detector.feed(bytes, true);
        let encoding = detector.guess(None, chardetng::Utf8Detection::Allow);
        let (text, _, _) = encoding.decode(bytes);
        text.into_owned()
    }
}
