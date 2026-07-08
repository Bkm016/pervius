//! 注解提取与格式化
//!
//! @author sky

use crate::class_structure::{AnnotationPair, EditableAnnotation};
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::{Annotation, AnnotationElement};

use super::{resolve_const, resolve_utf8};

/// Annotation → EditableAnnotation
pub(super) fn to_editable_annotation(ann: &Annotation, cp: &ConstantPool) -> EditableAnnotation {
    let type_desc = resolve_utf8(cp, ann.type_index);
    let elements = ann
        .elements
        .iter()
        .map(|pair| {
            let name = resolve_utf8(cp, pair.name_index);
            let (value, tag) = element_to_value(&pair.value, cp);
            AnnotationPair { name, value, tag }
        })
        .collect();
    EditableAnnotation {
        type_desc,
        elements,
    }
}

/// 将 AnnotationElement 转为 (格式化值, tag)
fn element_to_value(elem: &AnnotationElement, cp: &ConstantPool) -> (String, u8) {
    // 原始数值类型（统一用 resolve_const）
    if let Some(result) = ann_primitive!(elem, cp, resolve_const, [
        Byte => b'B', Char => b'C', Short => b'S', Int => b'I',
        Long => b'J', Float => b'F', Double => b'D',
    ]) {
        return result;
    }
    match elem {
        AnnotationElement::Boolean { const_value_index } => {
            let v = match cp.try_get_formatted_string(*const_value_index).as_deref() {
                Ok("0") => "false".to_string(),
                Ok("1") => "true".to_string(),
                Ok(v) => v.to_string(),
                Err(_) => format!("#{const_value_index}"),
            };
            (v, b'Z')
        }
        AnnotationElement::String { const_value_index } => {
            (resolve_utf8(cp, *const_value_index), b's')
        }
        AnnotationElement::Enum {
            type_name_index,
            const_name_index,
        } => {
            let type_name = resolve_utf8(cp, *type_name_index);
            let const_name = resolve_utf8(cp, *const_name_index);
            (format!("{type_name}.{const_name}"), b'e')
        }
        AnnotationElement::Class { class_info_index } => {
            (resolve_utf8(cp, *class_info_index), b'c')
        }
        AnnotationElement::Annotation { annotation } => {
            let inner = to_editable_annotation(annotation, cp);
            let desc = format_editable_annotation(&inner);
            (desc, b'@')
        }
        AnnotationElement::Array { values } => {
            let items: Vec<String> = values
                .iter()
                .map(|v| {
                    let (s, _) = element_to_value(v, cp);
                    s
                })
                .collect();
            (format!("{{{}}}", items.join(", ")), b'[')
        }
        // 原始数值类型已在 ann_primitive! 处理
        _ => (String::new(), 0),
    }
}

/// EditableAnnotation → 显示字符串（嵌套注解值用）
fn format_editable_annotation(ann: &EditableAnnotation) -> String {
    let clean = ann
        .type_desc
        .strip_prefix('L')
        .and_then(|s| s.strip_suffix(';'))
        .unwrap_or(&ann.type_desc);
    if ann.elements.is_empty() {
        format!("@{clean}")
    } else {
        let pairs: Vec<String> = ann
            .elements
            .iter()
            .map(|p| format!("{} = {}", p.name, p.value))
            .collect();
        format!("@{clean}({})", pairs.join(", "))
    }
}
