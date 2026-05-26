//! 字节码面板右侧详情编辑：class info / field / method 的属性编辑 + 注解 + 字节码
//!
//! @author sky

use crate::appearance::theme::flat_button_theme;
use crate::appearance::{codicon, theme};
use eframe::egui;
use egui_editor::search::FindMatch;
use egui_shell::components::FlatButton;
use pervius_java_bridge::bytecode::descriptor::{
    return_type_readable, short_class_name, short_descriptor, short_params,
};
use pervius_java_bridge::class_structure::{
    ClassStructure, EditableAnnotation, FieldInfo, MethodInfo,
};

/// 12px monospace 字体
fn mono_12() -> egui::FontId {
    egui::FontId::monospace(12.0)
}

/// 12px proportional 字体
fn prop_12() -> egui::FontId {
    egui::FontId::proportional(12.0)
}

/// monospace 12px label 简写
fn mono_label(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).color(color).font(mono_12()));
}

pub fn render_class_info_editable(
    ui: &mut egui::Ui,
    cs: &mut ClassStructure,
    read_only: bool,
) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("bc_detail")
        .auto_shrink(false)
        .show(ui, |ui| {
            // push_id 隔离 TextEdit undo 历史，避免与 Field/Method 面板共享 widget ID
            ui.push_id("class_info", |ui| {
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
                changed |= render_editable_kv(
                    ui,
                    "Access",
                    &mut cs.info.access,
                    theme::SYN_KEYWORD,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Name",
                    &mut cs.info.name,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Extends",
                    &mut cs.info.super_class,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                let mut ifaces_str = cs.info.interfaces.join(", ");
                let ifaces_changed = render_editable_kv(
                    ui,
                    "Implements",
                    &mut ifaces_str,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                if ifaces_changed {
                    cs.info.interfaces = ifaces_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    changed = true;
                }
                ui.add_space(8.0);
                changed |= render_annotations(ui, &mut cs.info.annotations, read_only);
                if cs.info.signature.is_some()
                    || cs.info.source_file.is_some()
                    || cs.info.is_deprecated
                    || !cs.info.version.is_empty()
                {
                    ui.add_space(12.0);
                    render_kv(ui, "Version", &cs.info.version);
                    if let Some(sig) = &cs.info.signature {
                        render_kv(ui, "Signature", sig);
                    }
                    if let Some(src) = &cs.info.source_file {
                        render_kv(ui, "Source", src);
                    }
                    if cs.info.is_deprecated {
                        render_kv(ui, "Deprecated", "true");
                    }
                }
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
            });
        });
    changed
}

pub fn render_field_editable(
    ui: &mut egui::Ui,
    field: &mut FieldInfo,
    idx: usize,
    read_only: bool,
) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("bc_detail")
        .auto_shrink(false)
        .show(ui, |ui| {
            // push_id 隔离 TextEdit undo 历史，避免与其他 Field/Method 共享 widget ID
            ui.push_id(("field", idx), |ui| {
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
                changed |= render_editable_kv(
                    ui,
                    "Access",
                    &mut field.access,
                    theme::SYN_KEYWORD,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Name",
                    &mut field.name,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Descriptor",
                    &mut field.descriptor,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                render_preview(ui, &format!("→ {}", short_descriptor(&field.descriptor)));
                if let Some(cv) = &mut field.constant_value {
                    changed |= render_editable_kv(ui, "Value", cv, theme::SYN_STRING, read_only);
                }
                ui.add_space(8.0);
                changed |= render_annotations(ui, &mut field.annotations, read_only);
                render_readonly_attrs(
                    ui,
                    &field.signature,
                    field.is_deprecated,
                    field.is_synthetic,
                );
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
            });
        });
    changed
}

pub fn render_method_editable(
    ui: &mut egui::Ui,
    method: &mut MethodInfo,
    idx: usize,
    matches: &[FindMatch],
    current: Option<usize>,
    scroll_to_line: &mut Option<usize>,
    read_only: bool,
) -> bool {
    let mut changed = false;
    egui::ScrollArea::both()
        .id_salt("bc_detail")
        .auto_shrink(false)
        .show(ui, |ui| {
            // push_id 隔离 TextEdit undo 历史，避免与其他 Method/Field 共享 widget ID
            ui.push_id(("method", idx), |ui| {
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
                changed |= render_editable_kv(
                    ui,
                    "Access",
                    &mut method.access,
                    theme::SYN_KEYWORD,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Name",
                    &mut method.name,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                changed |= render_editable_kv(
                    ui,
                    "Descriptor",
                    &mut method.descriptor,
                    theme::TEXT_PRIMARY,
                    read_only,
                );
                let ret = return_type_readable(&method.descriptor);
                let params = short_params(&method.descriptor);
                render_preview(ui, &format!("→ {ret} {}{params}", method.name));
                if !method.exceptions.is_empty() {
                    let throws = method
                        .exceptions
                        .iter()
                        .map(|e| short_class_name(e))
                        .collect::<Vec<_>>()
                        .join(", ");
                    render_kv(ui, "Throws", &throws);
                }
                ui.add_space(8.0);
                changed |= render_annotations(ui, &mut method.annotations, read_only);
                if method.has_code {
                    ui.add_space(12.0);
                    let t = theme::editor_theme();
                    let mut bc_cache = None;
                    let output = if read_only {
                        egui_editor::code_view::code_view_readonly_editable_style(
                            ui,
                            egui::Id::new(("bc_code", idx)),
                            &mut method.bytecode,
                            egui_editor::Language::Bytecode,
                            matches,
                            current,
                            &t,
                            &mut bc_cache,
                            None,
                            scroll_to_line,
                        )
                    } else {
                        egui_editor::code_view::code_view_editable(
                            ui,
                            egui::Id::new(("bc_code", idx)),
                            &mut method.bytecode,
                            egui_editor::Language::Bytecode,
                            matches,
                            current,
                            &t,
                            &mut bc_cache,
                            None,
                            scroll_to_line,
                        )
                    };
                    changed |= output.value;
                } else {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(theme::BYTECODE_DETAIL_PAD);
                        mono_label(ui, "// no code (abstract or native)", theme::TEXT_MUTED);
                    });
                }
                render_readonly_attrs(
                    ui,
                    &method.signature,
                    method.is_deprecated,
                    method.is_synthetic,
                );
                ui.add_space(theme::BYTECODE_DETAIL_PAD);
            });
        });
    changed
}

/// Kotlin 编译器内部注解，编辑无意义，只读展示
fn is_kotlin_internal_annotation(type_desc: &str) -> bool {
    matches!(
        type_desc,
        "Lkotlin/Metadata;" | "Lkotlin/jvm/internal/SourceDebugExtension;"
    )
}

/// 渲染注解列表：每行一个注解（类型 + 元素），可增删改。返回是否变更
fn render_annotations(
    ui: &mut egui::Ui,
    annotations: &mut Vec<EditableAnnotation>,
    read_only: bool,
) -> bool {
    let mut changed = false;
    let mut remove_idx = None;
    for (i, ann) in annotations.iter_mut().enumerate() {
        let readonly = read_only || is_kotlin_internal_annotation(&ann.type_desc);
        ui.horizontal(|ui| {
            ui.add_space(theme::BYTECODE_DETAIL_PAD);
            mono_label(ui, "@", theme::SYN_ANNOTATION);
            if readonly {
                mono_label(ui, &ann.type_desc, theme::TEXT_MUTED);
            } else {
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut ann.type_desc)
                            .font(mono_12())
                            .text_color(theme::SYN_ANNOTATION)
                            .desired_width(200.0)
                            .frame(egui::Frame::NONE),
                    )
                    .changed();
            }
            if !ann.elements.is_empty() {
                mono_label(ui, "(", theme::TEXT_MUTED);
                let last = ann.elements.len() - 1;
                for (j, elem) in ann.elements.iter_mut().enumerate() {
                    mono_label(ui, &format!("{} = ", elem.name), theme::TEXT_MUTED);
                    let color = match elem.tag {
                        b's' => theme::SYN_STRING,
                        b'I' | b'J' | b'F' | b'D' | b'B' | b'S' | b'C' => theme::SYN_NUMBER,
                        b'Z' => theme::SYN_KEYWORD,
                        _ => theme::SYN_TEXT,
                    };
                    if readonly {
                        mono_label(ui, &elem.value, color);
                    } else {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut elem.value)
                                    .font(mono_12())
                                    .text_color(color)
                                    .desired_width(120.0)
                                    .frame(egui::Frame::NONE),
                            )
                            .changed();
                    }
                    if j < last {
                        mono_label(ui, ", ", theme::TEXT_MUTED);
                    }
                }
                mono_label(ui, ")", theme::TEXT_MUTED);
            }
            if !readonly {
                let fbt = flat_button_theme(theme::TEXT_SECONDARY);
                if ui
                    .add(
                        FlatButton::new(codicon::CLOSE, &fbt)
                            .font_size(10.0)
                            .font_family(codicon::family())
                            .inactive_color(theme::TEXT_MUTED)
                            .min_size(egui::vec2(18.0, 18.0)),
                    )
                    .on_hover_text("Remove annotation")
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            }
        });
    }
    if let Some(idx) = remove_idx {
        annotations.remove(idx);
        changed = true;
    }
    if !read_only {
        ui.horizontal(|ui| {
            ui.add_space(theme::BYTECODE_DETAIL_PAD);
            let fbt = flat_button_theme(theme::TEXT_SECONDARY);
            if ui
                .add(
                    FlatButton::new("+ annotation", &fbt)
                        .font_size(11.0)
                        .inactive_color(theme::TEXT_MUTED)
                        .min_size(egui::vec2(0.0, 20.0)),
                )
                .clicked()
            {
                annotations.push(EditableAnnotation {
                    type_desc: String::new(),
                    elements: Vec::new(),
                });
                changed = true;
            }
        });
    }
    changed
}

/// 单行可编辑文本（无边框 monospace），返回 Response
fn styled_singleline(
    ui: &mut egui::Ui,
    text: &mut String,
    color: egui::Color32,
    font_size: f32,
) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .font(egui::FontId::monospace(font_size))
            .text_color(color)
            .desired_width(f32::INFINITY)
            .frame(egui::Frame::NONE),
    )
}

/// 渲染 signature / deprecated / synthetic 只读属性块
fn render_readonly_attrs(
    ui: &mut egui::Ui,
    signature: &Option<String>,
    is_deprecated: bool,
    is_synthetic: bool,
) {
    if signature.is_none() && !is_deprecated && !is_synthetic {
        return;
    }
    ui.add_space(12.0);
    if let Some(sig) = signature {
        render_kv(ui, "Signature", sig);
    }
    if is_deprecated {
        render_kv(ui, "Deprecated", "true");
    }
    if is_synthetic {
        render_kv(ui, "Synthetic", "true");
    }
}

/// 渲染 KV 行的 key label + 对齐间距
fn render_kv_label(ui: &mut egui::Ui, key: &str) {
    ui.add_space(theme::BYTECODE_DETAIL_PAD);
    if !key.is_empty() {
        ui.label(
            egui::RichText::new(key)
                .color(theme::TEXT_MUTED)
                .font(prop_12()),
        );
        let used = ui.min_rect().width();
        if used < theme::BYTECODE_KV_KEY_WIDTH {
            ui.add_space(theme::BYTECODE_KV_KEY_WIDTH - used);
        }
    } else {
        ui.add_space(theme::BYTECODE_KV_KEY_WIDTH);
    }
}

fn render_kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        render_kv_label(ui, key);
        ui.label(
            egui::RichText::new(value)
                .color(theme::TEXT_PRIMARY)
                .font(mono_12()),
        );
    });
}

/// key-value 行（value 可编辑），返回 value 是否变更
fn render_editable_kv(
    ui: &mut egui::Ui,
    key: &str,
    value: &mut String,
    color: egui::Color32,
    read_only: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        render_kv_label(ui, key);
        if read_only {
            ui.label(
                egui::RichText::new(value.as_str())
                    .color(color)
                    .font(egui::FontId::monospace(13.0)),
            );
        } else {
            changed = styled_singleline(ui, value, color, 13.0).changed();
        }
    });
    changed
}

/// 可读预览行（与 KV value 列对齐）
fn render_preview(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(theme::BYTECODE_KV_KEY_WIDTH);
        ui.label(
            egui::RichText::new(text)
                .color(theme::TEXT_MUTED)
                .font(egui::FontId::proportional(11.0)),
        );
    });
}
