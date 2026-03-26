//! Spring 方法参数（FR-03）与 `@RequestBody` JSON 模板（FR-04）。

use tree_sitter::Node;

use crate::model::{ApiParam, ParamLocation};

use super::class_fields::FieldCatalog;
use super::parse::{
    annotation_binding_name, annotation_bool_attr, annotation_simple_name, annotation_string_attr,
    modifier_annotations, modifiers_node, node_text,
};

/// `@RequestBody` 的 JSON 模板是启动时生成（Eager）还是延后到选中接口再解析（Deferred）。
#[derive(Debug, Clone, Copy, Default)]
pub enum BodyTemplatePolicy {
    #[default]
    Eager,
    Deferred,
}

#[derive(Debug, Default)]
pub struct ExtractedParams {
    pub path_params: Vec<ApiParam>,
    pub query_params: Vec<ApiParam>,
    pub headers: Vec<ApiParam>,
    pub cookie_params: Vec<ApiParam>,
    pub model_params: Vec<ApiParam>,
    pub body_binding: Option<ApiParam>,
    pub body: Option<serde_json::Value>,
}

pub fn extract_method_parameters(
    method: Node,
    source: &[u8],
    field_catalog: &FieldCatalog,
    body_policy: BodyTemplatePolicy,
) -> ExtractedParams {
    let mut out = ExtractedParams::default();
    let Some(fp) = method.child_by_field_name("parameters") else {
        return out;
    };

    for i in 0..fp.named_child_count() {
        let Some(p) = fp.named_child(i) else {
            continue;
        };
        if p.kind() != "formal_parameter" {
            continue;
        }
        consume_formal_parameter(p, source, &mut out, field_catalog, body_policy);
    }
    out
}

fn consume_formal_parameter(
    p: Node,
    source: &[u8],
    out: &mut ExtractedParams,
    field_catalog: &FieldCatalog,
    body_policy: BodyTemplatePolicy,
) {
    let Some(java_type) = p.child_by_field_name("type") else {
        return;
    };
    let java_type_str = node_text(java_type, source);

    let param_name = p
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("_")
        .to_string();

    let mods = modifiers_node(p);
    let anns = mods.map(modifier_annotations).unwrap_or_default();

    let mut handled = false;

    for ann in &anns {
        let Some(simple) = annotation_simple_name(*ann, source) else {
            continue;
        };
        match simple.as_str() {
            "PathVariable" => {
                let name = annotation_binding_name(*ann, source, &param_name);
                out.path_params.push(ApiParam {
                    name,
                    java_type: java_type_str.clone(),
                    location: ParamLocation::Path,
                    required: annotation_bool_attr(*ann, source, "required").unwrap_or(true),
                    default_value: None,
                });
                handled = true;
                break;
            }
            "RequestParam" => {
                let name = annotation_binding_name(*ann, source, &param_name);
                let def = annotation_string_attr(*ann, source, "defaultValue");
                let required = match annotation_bool_attr(*ann, source, "required") {
                    Some(r) => r,
                    None => def.is_none(),
                };
                out.query_params.push(ApiParam {
                    name,
                    java_type: java_type_str.clone(),
                    location: ParamLocation::Query,
                    required,
                    default_value: def,
                });
                handled = true;
                break;
            }
            "RequestHeader" => {
                let name = annotation_binding_name(*ann, source, &param_name);
                let def = annotation_string_attr(*ann, source, "defaultValue");
                let required = annotation_bool_attr(*ann, source, "required").unwrap_or(true);
                out.headers.push(ApiParam {
                    name,
                    java_type: java_type_str.clone(),
                    location: ParamLocation::Header,
                    required,
                    default_value: def,
                });
                handled = true;
                break;
            }
            "CookieValue" => {
                let name = annotation_binding_name(*ann, source, &param_name);
                let def = annotation_string_attr(*ann, source, "defaultValue");
                let required = annotation_bool_attr(*ann, source, "required").unwrap_or(true);
                out.cookie_params.push(ApiParam {
                    name,
                    java_type: java_type_str.clone(),
                    location: ParamLocation::Cookie,
                    required,
                    default_value: def,
                });
                handled = true;
                break;
            }
            "RequestBody" => {
                if out.body.is_none() && out.body_binding.is_none() {
                    out.body_binding = Some(ApiParam {
                        name: param_name.clone(),
                        java_type: java_type_str.clone(),
                        location: ParamLocation::Body,
                        required: annotation_bool_attr(*ann, source, "required").unwrap_or(true),
                        default_value: None,
                    });
                    out.body = match body_policy {
                        BodyTemplatePolicy::Eager => {
                            Some(java_type_to_json_template(&java_type_str, field_catalog))
                        }
                        BodyTemplatePolicy::Deferred => None,
                    };
                }
                handled = true;
                break;
            }
            "ModelAttribute" => {
                let name = annotation_binding_name(*ann, source, &param_name);
                out.model_params.push(ApiParam {
                    name,
                    java_type: java_type_str.clone(),
                    location: ParamLocation::Model,
                    required: true,
                    default_value: None,
                });
                handled = true;
                break;
            }
            _ => {}
        }
    }

    // Spring：参数无绑定类注解时，仍按 `@RequestParam` 解析。
    if !handled {
        out.query_params.push(ApiParam {
            name: param_name,
            java_type: java_type_str,
            location: ParamLocation::Query,
            required: true,
            default_value: None,
        });
    }
}

// --- FR-04 ---

const MAX_TYPE_DEPTH: usize = 32;

fn java_type_to_json_template(java_type: &str, catalog: &FieldCatalog) -> serde_json::Value {
    java_type_to_json_inner(java_type.trim(), 0, catalog).unwrap_or(serde_json::Value::Null)
}

/// 由已收集的 `FieldCatalog` 生成 `@RequestBody` JSON 模板（懒加载解析用）。
pub fn request_body_json_template(java_type: &str, catalog: &FieldCatalog) -> serde_json::Value {
    java_type_to_json_template(java_type, catalog)
}

fn java_type_to_json_inner(
    t: &str,
    depth: usize,
    catalog: &FieldCatalog,
) -> Option<serde_json::Value> {
    if depth > MAX_TYPE_DEPTH {
        return None;
    }
    let t: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() {
        return Some(serde_json::Value::Null);
    }

    if let Some(inner) = strip_wrapped_generic(&t, "Optional<") {
        return java_type_to_json_inner(inner, depth + 1, catalog);
    }
    if let Some(inner) = strip_wrapped_generic(&t, "java.util.Optional<") {
        return java_type_to_json_inner(inner, depth + 1, catalog);
    }

    if strip_wrapped_generic(&t, "List<").is_some() || strip_wrapped_generic(&t, "java.util.List<").is_some()
    {
        return Some(serde_json::Value::Array(vec![]));
    }

    if strip_wrapped_generic(&t, "Set<").is_some() {
        return Some(serde_json::Value::Array(vec![]));
    }

    if t.strip_prefix("Map<").is_some() || t.strip_prefix("java.util.Map<").is_some() {
        return Some(serde_json::Value::Object(serde_json::Map::new()));
    }

    if t.ends_with("[]") {
        return Some(serde_json::Value::Array(vec![]));
    }

    let simple = t.rsplit('.').next().unwrap_or(&t);
    let simple_base = simple.split('<').next().unwrap_or(simple).trim();

    Some(match simple_base {
        "String" | "CharSequence" | "UUID" | "LocalDate" | "LocalDateTime" | "Instant"
        | "ZonedDateTime" | "OffsetDateTime" | "char" | "Character" => serde_json::Value::Null,
        "Integer" | "int" | "Long" | "long" | "Short" | "short" | "Byte" | "byte"
        | "BigInteger" | "BigDecimal" => serde_json::Value::Null,
        "Double" | "double" | "Float" | "float" => serde_json::Value::Null,
        "Boolean" | "boolean" => serde_json::Value::Null,
        "void" | "Void" => serde_json::Value::Null,
        name if name.chars().next().is_some_and(|c| c.is_uppercase()) => {
            if let Some(fields) = catalog.get(name) {
                let mut m = serde_json::Map::new();
                for (fname, ftype) in fields {
                    if let Some(v) = java_type_to_json_inner(ftype.as_str(), depth + 1, catalog) {
                        m.insert(fname.clone(), v);
                    }
                }
                serde_json::Value::Object(m)
            } else {
                serde_json::json!({})
            }
        }
        _ => serde_json::Value::Null,
    })
}

fn strip_wrapped_generic<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(prefix)?;
    strip_matching_angle(rest)
}

fn strip_matching_angle(s: &str) -> Option<&str> {
    let mut depth = 1usize;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[..i].trim());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_template_primitives() {
        let empty = FieldCatalog::new();
        assert_eq!(
            java_type_to_json_template("String", &empty),
            serde_json::Value::Null
        );
        assert_eq!(
            java_type_to_json_template("int", &empty),
            serde_json::Value::Null
        );
        assert_eq!(
            java_type_to_json_template("boolean", &empty),
            serde_json::Value::Null
        );
    }

    #[test]
    fn json_template_generics() {
        let empty = FieldCatalog::new();
        assert_eq!(
            java_type_to_json_template("List<String>", &empty),
            serde_json::json!([])
        );
        assert_eq!(
            java_type_to_json_template("Optional<UserDto>", &empty),
            serde_json::json!({})
        );
    }

    #[test]
    fn json_template_dto_from_catalog() {
        let mut cat = FieldCatalog::new();
        cat.insert(
            "UserDto".into(),
            vec![("id".into(), "long".into()), ("name".into(), "String".into())],
        );
        let v = java_type_to_json_template("UserDto", &cat);
        assert_eq!(v, serde_json::json!({"id": null, "name": null}));
    }
}
