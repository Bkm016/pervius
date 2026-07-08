//! 字段级别元数据提取
//!
//! @author sky

use super::common::extract_common_attrs;
use super::resolve_utf8;
use crate::class_structure::FieldInfo;
use ristretto_classfile::ConstantPool;
use ristretto_classfile::attributes::Attribute;

/// 从 Field 提取字段信息（访问标记、名称、描述符、常量值、注解等）
pub(super) fn extract_field(field: &ristretto_classfile::Field, cp: &ConstantPool) -> FieldInfo {
    let access = field.access_flags.as_code().to_string();
    let name = resolve_utf8(cp, field.name_index);
    let descriptor = resolve_utf8(cp, field.descriptor_index);
    let common = extract_common_attrs(&field.attributes, cp);
    let mut constant_value = None;
    for attr in &field.attributes {
        if let Attribute::ConstantValue {
            constant_value_index,
            ..
        } = attr
        {
            constant_value = cp.try_get_formatted_string(*constant_value_index).ok();
        }
    }
    FieldInfo {
        access,
        name,
        descriptor,
        constant_value,
        signature: common.signature,
        annotations: common.annotations,
        is_deprecated: common.is_deprecated,
        is_synthetic: common.is_synthetic,
        modified: false,
        saved: false,
    }
}
