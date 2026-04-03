//! Spring Web 映射注解解析（启发式，覆盖常见写法）。

use tree_sitter::Node;

use super::parse::{annotation_simple_name, first_string_literal};

#[derive(Debug, Clone)]
pub struct MappingInfo {
    pub http_method: String,
    /// 方法级 path 片段（未与类级合并）。
    pub path_suffix: String,
}

/// 从方法上的映射注解解析 HTTP 方法与 path；若无映射则 `None`。
pub fn mapping_from_method_annotations(annotations: &[Node], source: &[u8]) -> Option<MappingInfo> {
    for ann in annotations {
        let simple = annotation_simple_name(*ann, source)?;
        match simple.as_str() {
            "GetMapping" => {
                return Some(MappingInfo {
                    http_method: "GET".into(),
                    path_suffix: path_from_mapping_annotation(*ann, source),
                });
            }
            "PostMapping" => {
                return Some(MappingInfo {
                    http_method: "POST".into(),
                    path_suffix: path_from_mapping_annotation(*ann, source),
                });
            }
            "PutMapping" => {
                return Some(MappingInfo {
                    http_method: "PUT".into(),
                    path_suffix: path_from_mapping_annotation(*ann, source),
                });
            }
            "DeleteMapping" => {
                return Some(MappingInfo {
                    http_method: "DELETE".into(),
                    path_suffix: path_from_mapping_annotation(*ann, source),
                });
            }
            "PatchMapping" => {
                return Some(MappingInfo {
                    http_method: "PATCH".into(),
                    path_suffix: path_from_mapping_annotation(*ann, source),
                });
            }
            "RequestMapping" => {
                return parse_request_mapping(*ann, source);
            }
            _ => {}
        }
    }
    None
}

/// 类级 `@RequestMapping` 的 path（`value` / `path` 或第一个字符串参数）。
pub fn class_level_request_mapping_path(annotations: &[Node], source: &[u8]) -> String {
    for ann in annotations {
        let Some(simple) = annotation_simple_name(*ann, source) else {
            continue;
        };
        if simple == "RequestMapping" {
            return path_from_request_mapping_annotation(*ann, source);
        }
    }
    String::new()
}

fn path_from_mapping_annotation(ann: Node, source: &[u8]) -> String {
    if ann.kind() == "marker_annotation" {
        return String::new();
    }
    first_string_literal(ann, source).unwrap_or_default()
}

fn path_from_request_mapping_annotation(ann: Node, source: &[u8]) -> String {
    if ann.kind() == "marker_annotation" {
        return String::new();
    }
    // 优先命名参数 value / path
    if let Some(p) = request_mapping_named_string(ann, source, "value") {
        return p;
    }
    if let Some(p) = request_mapping_named_string(ann, source, "path") {
        return p;
    }
    // 第一个字符串字面量（常见于 @RequestMapping("/a")）
    first_string_literal(ann, source).unwrap_or_default()
}

fn parse_request_mapping(ann: Node, source: &[u8]) -> Option<MappingInfo> {
    let path_suffix = path_from_request_mapping_annotation(ann, source);
    let http_method = if ann.kind() == "marker_annotation" {
        "ALL".to_string()
    } else {
        request_mapping_http_method(ann, source).unwrap_or_else(|| "ALL".to_string())
    };
    Some(MappingInfo {
        http_method,
        path_suffix,
    })
}

/// 从 `method = RequestMethod.GET` 或 `method = GET` 等提取动词。
fn request_mapping_http_method(ann: Node, source: &[u8]) -> Option<String> {
    let args = ann.child_by_field_name("arguments")?;
    for i in 0..args.named_child_count() {
        let child = args.named_child(i)?;
        if child.kind() != "element_value_pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let key_text = key.utf8_text(source).ok()?;
        if key_text != "method" {
            continue;
        }
        let value = child.child_by_field_name("value")?;
        return extract_method_constant(value, source);
    }
    None
}

fn request_mapping_named_string(ann: Node, source: &[u8], name: &str) -> Option<String> {
    let args = ann.child_by_field_name("arguments")?;
    for i in 0..args.named_child_count() {
        let child = args.named_child(i)?;
        if child.kind() != "element_value_pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let key_text = key.utf8_text(source).ok()?;
        if key_text != name {
            continue;
        }
        let value = child.child_by_field_name("value")?;
        if let Some(s) = first_string_literal(value, source) {
            return Some(s);
        }
    }
    None
}

fn extract_method_constant(expr: Node, source: &[u8]) -> Option<String> {
    let text = expr.utf8_text(source).ok()?;
    // `RequestMethod.GET` 或带空格
    for m in [
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE",
    ] {
        if text.contains(m) {
            return Some(m.to_string());
        }
    }
    None
}

/// 启发式：无这些片段的文件几乎不可能含 Spring MVC 控制器，可跳过 tree-sitter 解析。
/// （漏检情形：仅通过元注解继承的控制器等，极少见。）
pub fn source_might_contain_spring_web_controller(source: &str) -> bool {
    if source.contains("@RestController") {
        return true;
    }
    for (idx, _) in source.match_indices("@Controller") {
        let after = &source[idx + "@Controller".len()..];
        if after.starts_with("Advice") {
            continue;
        }
        return true;
    }
    false
}

/// 是否为 Spring MVC 控制器类（`@RestController` / `@Controller`）。
pub fn is_controller_class(annotations: &[Node], source: &[u8]) -> bool {
    for ann in annotations {
        let Some(simple) = annotation_simple_name(*ann, source) else {
            continue;
        };
        if simple == "RestController" || simple == "Controller" {
            return true;
        }
    }
    false
}

/// 合并类级与方法级 path。
pub fn join_spring_paths(class_prefix: &str, method_suffix: &str) -> String {
    let mut base = class_prefix.trim().to_string();
    if base.is_empty() {
        base = String::new();
    } else if !base.starts_with('/') {
        base = format!("/{}", base.trim_start_matches('/'));
    }
    let suf = method_suffix.trim();
    if suf.is_empty() {
        return if base.is_empty() {
            "/".to_string()
        } else {
            base
        };
    }
    let suf = if suf.starts_with('/') {
        suf.to_string()
    } else {
        format!("/{suf}")
    };
    if base.is_empty() {
        suf
    } else {
        format!("{}{}", base.trim_end_matches('/'), suf)
    }
}

#[cfg(test)]
mod tests {
    use super::source_might_contain_spring_web_controller;

    #[test]
    fn prefilter_rest_controller() {
        assert!(source_might_contain_spring_web_controller(
            "import x;\n@RestController\nclass C {}"
        ));
    }

    #[test]
    fn prefilter_controller_not_advice() {
        assert!(source_might_contain_spring_web_controller(
            "@Controller\nclass C {}"
        ));
    }

    #[test]
    fn prefilter_skips_controller_advice() {
        assert!(!source_might_contain_spring_web_controller(
            "@ControllerAdvice\nclass A {}"
        ));
    }

    #[test]
    fn prefilter_skips_plain_service() {
        assert!(!source_might_contain_spring_web_controller(
            "@Service\nclass S { void x() {} }"
        ));
    }
}
