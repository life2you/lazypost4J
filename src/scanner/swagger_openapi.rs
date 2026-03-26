//! OpenAPI / Swagger 注解（springdoc、swagger-annotations）中的文案提取。

use tree_sitter::Node;

use super::parse::{
    annotation_simple_name, annotation_string_attr, modifier_annotations, modifiers_node,
};

/// 类上的 `@Tag(name = "...")`（OpenAPI 3）。
pub fn class_tag_name(class_anns: &[Node], source: &[u8]) -> Option<String> {
    for ann in class_anns {
        let simple = annotation_simple_name(*ann, source)?;
        if simple != "Tag" {
            continue;
        }
        let n = annotation_string_attr(*ann, source, "name")?;
        if !n.is_empty() {
            return Some(n);
        }
    }
    None
}

/// 方法上的 `@Operation`（OAS3）或 `@ApiOperation`（Swagger2）摘要文案。
pub fn method_operation_summary(method: Node, source: &[u8]) -> Option<String> {
    let mods = modifiers_node(method)?;
    let anns = modifier_annotations(mods);
    for ann in &anns {
        let simple = annotation_simple_name(*ann, source)?;
        if simple == "Operation" {
            for key in ["summary", "description", "value"] {
                if let Some(v) = annotation_string_attr(*ann, source, key) {
                    if !v.trim().is_empty() {
                        return Some(v);
                    }
                }
            }
        } else if simple == "ApiOperation" {
            for key in ["value", "notes", "summary"] {
                if let Some(v) = annotation_string_attr(*ann, source, key) {
                    if !v.trim().is_empty() {
                        return Some(v);
                    }
                }
            }
        }
    }
    None
}
