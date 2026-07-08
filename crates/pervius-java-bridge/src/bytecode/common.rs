//! 公共属性提取（signature / deprecated / synthetic / annotations）
//!
//! class、field、method 三者共享的元数据提取逻辑。
//!
//! @author sky

use super::annotation::to_editable_annotation;
use super::resolve_utf8;
use crate::class_structure::EditableAnnotation;
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Attribute;

/// 从属性列表提取公共元数据（signature / deprecated / synthetic / annotations）
pub(super) struct CommonAttrs {
    /// 泛型签名
    pub signature: Option<String>,
    /// 注解列表
    pub annotations: Vec<EditableAnnotation>,
    /// 是否标记 Deprecated
    pub is_deprecated: bool,
    /// 是否编译器生成
    pub is_synthetic: bool,
}

/// 遍历属性列表，提取 Signature / Deprecated / Synthetic / RuntimeVisibleAnnotations
pub(super) fn extract_common_attrs(attrs: &[Attribute], cp: &ConstantPool) -> CommonAttrs {
    let mut result = CommonAttrs {
        signature: None,
        annotations: Vec::new(),
        is_deprecated: false,
        is_synthetic: false,
    };
    for attr in attrs {
        match attr {
            Attribute::Signature {
                signature_index, ..
            } => {
                result.signature = cp
                    .try_get_utf8(*signature_index)
                    .ok()
                    .map(|s| s.to_string());
            }
            Attribute::Deprecated { .. } => result.is_deprecated = true,
            Attribute::Synthetic { .. } => result.is_synthetic = true,
            // RuntimeInvisibleAnnotations 不读取——Kotlin @Metadata 的 d1 字段含
            // protobuf 二进制数据，走一趟文本编辑器 round-trip 必坏
            Attribute::RuntimeVisibleAnnotations {
                annotations: anns, ..
            } => {
                for ann in anns {
                    let type_desc = resolve_utf8(cp, ann.type_index);
                    if crate::KOTLIN_INTERNAL_ANNOTATIONS.contains(&type_desc.as_str()) {
                        continue;
                    }
                    result.annotations.push(to_editable_annotation(ann, cp));
                }
            }
            _ => {}
        }
    }
    result
}
