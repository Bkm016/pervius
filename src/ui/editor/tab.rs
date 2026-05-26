//! 编辑器 Tab 数据结构
//!
//! @author sky

use super::view_toggle::ActiveView;
use egui_editor::highlight::{self, Language, Span};
use egui_editor::{EditableLayoutCache, LayoutCache};
use egui_hex_view::HexViewState;
use pervius_java_bridge::class_structure::ClassStructure;
use pervius_java_bridge::compiler::CompileDiagnostic;
use rust_i18n::t;
use std::path::{Path, PathBuf};

tabookit::class! {
    /// 预处理后的代码视图数据（虚拟滚动用）
    pub struct CodeData {
        /// 语法高亮 span（全文字节偏移，已排序）
        pub spans: Vec<Span>,
        /// 每行在源码中的起始字节偏移
        pub line_starts: Vec<usize>,
    }

    pub fn new(source: &str, spans: Vec<Span>) -> Self {
        let line_starts = highlight::compute_line_starts(source);
        Self { spans, line_starts }
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// 反编译源码编辑模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceEditMode {
    Locked,
    Unlocked,
}

/// 字节码面板导航选中状态
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BytecodeSelection {
    ClassInfo,
    Field(usize),
    Method(usize),
}

tabookit::class! {
    /// 编辑器 Tab 数据
    pub struct EditorTab {
        pub title: String,
        /// JAR 内条目路径（用于 tab 去重 + widget ID）
        pub entry_path: Option<String>,
        /// 独立文件的磁盘路径（非 JAR 内条目，保存时直接写回磁盘，不参与 JAR modified 管理）
        pub standalone_path: Option<PathBuf>,
        /// 反编译源码（只读）
        pub decompiled: String,
        /// 原始字节（只读，用于 hex 视图）
        pub raw_bytes: Vec<u8>,
        pub language: Language,
        pub active_view: ActiveView,
        /// .class 文件才显示三视图切换
        pub is_class: bool,
        /// 纯文本文件（可直接编辑）
        pub is_text: bool,
        /// class 文件版本信息（如 "Java 21 (class 65.0)"）
        pub class_info: Option<String>,
        pub is_modified: bool,
        /// class 反编译源码编辑模式
        pub source_edit: SourceEditMode,
        /// 反编译源码是否有未保存修改（独立于结构化编辑）
        pub source_modified: bool,
        /// 进入源码编辑模式时的快照，供重置使用
        pub original_source: String,
        /// 最近一次编译诊断
        pub compile_diagnostics: Vec<CompileDiagnostic>,
        /// Hex 视图交互状态
        pub hex_state: HexViewState,
        /// 反编译视图预处理数据
        pub(super) decompiled_data: CodeData,
        /// 反编译行 → 原始源码行号映射（1-indexed），None 表示无映射
        pub decompiled_line_mapping: Vec<Option<u32>>,
        /// 解析后的 class 结构化数据
        pub class_structure: Option<ClassStructure>,
        /// 字节码面板当前选中项
        pub bc_selection: BytecodeSelection,
        /// 字节码面板左侧导航栏宽度（可拖拽调整）
        pub nav_width: f32,
        /// 只读视图布局缓存（避免每帧重建 LayoutJob）
        pub layout_cache: Option<LayoutCache>,
        /// 可编辑视图布局缓存（缓存 tree-sitter span + galley）
        pub editable_layout_cache: Option<EditableLayoutCache>,
        /// 视窗模式手动覆盖（None = 自动检测，Some(true) = 强制启用，Some(false) = 强制关闭）
        pub viewport_override: Option<bool>,
        /// 待滚动到的目标行号（0-based），设置后下一帧动画滚动到该行
        pub pending_scroll_to_line: Option<usize>,
    }

    /// 创建 .class 文件 tab
    pub fn new_class(
        title: impl Into<String>,
        entry_path: impl Into<String>,
        raw_bytes: Vec<u8>,
        language: Language,
    ) -> Self {
        let cs = pervius_java_bridge::bytecode::disassemble(&raw_bytes).ok();
        let class_info = cs.as_ref().map(|c| c.info.version.clone());
        let bc_selection = cs
            .as_ref()
            .map(|c| {
                if c.methods.is_empty() {
                    BytecodeSelection::ClassInfo
                } else {
                    BytecodeSelection::Method(0)
                }
            })
            .unwrap_or(BytecodeSelection::ClassInfo);
        let decompiled = t!("editor.decompiler_placeholder").to_string();
        let dec_data = CodeData::new(&decompiled, highlight::compute_spans(&decompiled, language));
        Self {
            title: title.into(),
            entry_path: Some(entry_path.into()),
            standalone_path: None,
            decompiled: decompiled.clone(),
            raw_bytes,
            language,
            active_view: ActiveView::Hex,
            is_class: true,
            is_text: false,
            class_info,
            is_modified: false,
            source_edit: SourceEditMode::Locked,
            source_modified: false,
            original_source: decompiled.clone(),
            compile_diagnostics: Vec::new(),
            hex_state: HexViewState::default(),
            decompiled_data: dec_data,
            decompiled_line_mapping: Vec::new(),
            class_structure: cs,
            bc_selection,
            nav_width: 220.0,
            layout_cache: None,
            editable_layout_cache: None,
            viewport_override: None,
            pending_scroll_to_line: None,
        }
    }

    /// 创建文本文件 tab（默认 Decompiled 视图显示文本内容）
    pub fn new_text(
        title: impl Into<String>,
        entry_path: impl Into<String>,
        text: String,
        raw_bytes: Vec<u8>,
        language: Language,
    ) -> Self {
        let dec_data = CodeData::new(&text, highlight::compute_spans(&text, language));
        Self {
            title: title.into(),
            entry_path: Some(entry_path.into()),
            standalone_path: None,
            decompiled: text.clone(),
            raw_bytes,
            language,
            active_view: ActiveView::Decompiled,
            is_class: false,
            is_text: true,
            class_info: None,
            is_modified: false,
            source_edit: SourceEditMode::Locked,
            source_modified: false,
            original_source: text.clone(),
            compile_diagnostics: Vec::new(),
            hex_state: HexViewState::default(),
            decompiled_data: dec_data,
            decompiled_line_mapping: Vec::new(),
            class_structure: None,
            bc_selection: BytecodeSelection::ClassInfo,
            nav_width: 220.0,
            layout_cache: None,
            editable_layout_cache: None,
            viewport_override: None,
            pending_scroll_to_line: None,
        }
    }

    /// 创建二进制文件 tab（直接显示 hex 视图）
    pub fn new_binary(
        title: impl Into<String>,
        entry_path: impl Into<String>,
        raw_bytes: Vec<u8>,
    ) -> Self {
        Self {
            title: title.into(),
            entry_path: Some(entry_path.into()),
            standalone_path: None,
            decompiled: String::new(),
            raw_bytes,
            language: Language::Plain,
            active_view: ActiveView::Hex,
            is_class: false,
            is_text: false,
            class_info: None,
            is_modified: false,
            source_edit: SourceEditMode::Locked,
            source_modified: false,
            original_source: String::new(),
            compile_diagnostics: Vec::new(),
            hex_state: HexViewState::default(),
            decompiled_data: CodeData::new("", vec![]),
            decompiled_line_mapping: Vec::new(),
            class_structure: None,
            bc_selection: BytecodeSelection::ClassInfo,
            nav_width: 220.0,
            layout_cache: None,
            editable_layout_cache: None,
            viewport_override: None,
            pending_scroll_to_line: None,
        }
    }

    /// 更新反编译源码（反编译完成后调用）
    pub fn set_decompiled(
        &mut self,
        source: String,
        lang: Language,
        line_mapping: Vec<Option<u32>>,
    ) {
        self.decompiled_data = CodeData::new(&source, highlight::compute_spans(&source, lang));
        self.original_source = source.clone();
        self.decompiled = source;
        self.language = lang;
        self.decompiled_line_mapping = line_mapping;
        if !self.source_modified {
            self.compile_diagnostics.clear();
        }
        self.layout_cache = None;
        self.editable_layout_cache = None;
    }

    /// 当前选中方法的字节码文本（find bar 用）
    pub fn selected_bytecode_text(&self) -> &str {
        if let (Some(cs), BytecodeSelection::Method(idx)) =
            (&self.class_structure, self.bc_selection)
        {
            cs.methods
                .get(idx)
                .map(|m| m.bytecode.as_str())
                .unwrap_or("")
        } else {
            ""
        }
    }

    /// 是否正在编辑 class 反编译源码
    pub fn is_source_unlocked(&self) -> bool {
        self.source_edit == SourceEditMode::Unlocked
    }

    /// 进入源码编辑模式
    pub fn unlock_source_edit(&mut self) {
        self.source_edit = SourceEditMode::Unlocked;
        self.original_source = self.decompiled.clone();
        self.compile_diagnostics.clear();
    }

    /// 锁定源码编辑模式
    pub fn lock_source_edit(&mut self) {
        self.source_edit = SourceEditMode::Locked;
    }

    /// 丢弃源码编辑改动
    pub fn discard_source_changes(&mut self) {
        self.decompiled = self.original_source.clone();
        self.source_modified = false;
        self.compile_diagnostics.clear();
        self.refresh_decompiled_data();
    }

    /// 重建反编译/文本视图的高亮数据（文本编辑后调用）
    pub fn refresh_decompiled_data(&mut self) {
        self.decompiled_data = CodeData::new(
            &self.decompiled,
            highlight::compute_spans(&self.decompiled, self.language),
        );
        self.layout_cache = None;
        self.editable_layout_cache = None;
    }

    /// 根据 tab 类型序列化当前编辑内容为字节
    pub fn serialize_bytes(&self, jar_path: Option<&Path>) -> Result<Vec<u8>, String> {
        if self.is_text {
            return Ok(self.decompiled.as_bytes().to_vec());
        }
        if self.is_class {
            let cs = self.class_structure.as_ref().ok_or("no class structure")?;
            return pervius_java_bridge::save::apply_structure(&self.raw_bytes, cs, jar_path)
                .map_err(|e| e.to_string());
        }
        Ok(self.raw_bytes.clone())
    }

    /// 保存成功后更新 tab 状态：写入新字节、清除修改标记、重建 class structure
    pub fn commit_save(&mut self, new_bytes: Vec<u8>) {
        self.raw_bytes = new_bytes;
        self.is_modified = false;
        if self.is_class {
            if let Ok(new_cs) = pervius_java_bridge::bytecode::disassemble(&self.raw_bytes) {
                self.class_structure = Some(new_cs);
            }
        }
    }
}
