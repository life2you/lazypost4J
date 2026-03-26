//! 从源码收集「简单类名 → 字段列表」，用于 `@RequestBody` DTO 的 JSON 模板（避免 `{}` 需手写字段名）。

use std::collections::HashMap;

use tree_sitter::Node;

use super::parse::node_text;

pub type FieldCatalog = HashMap<String, Vec<(String, String)>>;

/// 遍历编译单元，收集每个 `class` 的成员字段（`field_declaration`）。
pub fn collect_field_catalog(root: Node, source: &[u8]) -> FieldCatalog {
    let mut map = FieldCatalog::new();
    collect_recursive(root, source, &mut map);
    map
}

fn collect_recursive(node: Node, source: &[u8], map: &mut FieldCatalog) {
    match node.kind() {
        "class_declaration" => {
            if let Some(name) = type_simple_name(node, source) {
                let fields = collect_class_instance_fields(node, source);
                if !fields.is_empty() {
                    map.insert(name, fields);
                }
            }
        }
        "record_declaration" => {
            if let Some(name) = type_simple_name(node, source) {
                let fields = collect_record_components(node, source);
                if !fields.is_empty() {
                    map.insert(name, fields);
                }
            }
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            collect_recursive(c, source, map);
        }
    }
}

fn type_simple_name(decl: Node, source: &[u8]) -> Option<String> {
    let id = decl.child_by_field_name("name")?;
    id.utf8_text(source).ok().map(|s| s.to_string())
}

/// `record Foo(String a, int b)` 的组件在头部 `formal_parameters` 里，而非 `field_declaration`。
fn collect_record_components(record: Node, source: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(fp) = record.child_by_field_name("parameters") else {
        return out;
    };
    for i in 0..fp.named_child_count() {
        let Some(p) = fp.named_child(i) else {
            continue;
        };
        if p.kind() != "formal_parameter" {
            continue;
        }
        let Some(ty) = p.child_by_field_name("type") else {
            continue;
        };
        let java_type = node_text(ty, source);
        let param_name = p
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("_")
            .to_string();
        out.push((param_name, java_type));
    }
    out
}

fn collect_class_instance_fields(class: Node, source: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(body) = class.child_by_field_name("body") else {
        return out;
    };
    for i in 0..body.child_count() {
        let Some(ch) = body.child(i) else {
            continue;
        };
        if ch.kind() != "field_declaration" {
            continue;
        }
        let Some(ty) = ch.child_by_field_name("type") else {
            continue;
        };
        let java_type = node_text(ty, source);
        for j in 0..ch.child_count() {
            let Some(c) = ch.child(j) else {
                continue;
            };
            if c.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_n) = c.child_by_field_name("name") else {
                continue;
            };
            if let Ok(name) = name_n.utf8_text(source) {
                out.push((name.to_string(), java_type.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn catalog_finds_echo_body_field() {
        let src = r#"
package demo;
class DemoController {
    public static class EchoBody {
        public String text;
    }
}
"#;
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let cat = collect_field_catalog(tree.root_node(), src.as_bytes());
        let fields = cat.get("EchoBody").expect("EchoBody");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "text");
        assert!(fields[0].1.contains("String"), "{}", fields[0].1);
    }

    #[test]
    fn catalog_finds_record_components() {
        let src = r#"
package demo;
record EchoBody(String text, int code) {}
"#;
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let cat = collect_field_catalog(tree.root_node(), src.as_bytes());
        let fields = cat.get("EchoBody").expect("EchoBody record");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "text");
        assert!(fields[0].1.contains("String"), "{}", fields[0].1);
        assert_eq!(fields[1].0, "code");
        assert!(fields[1].1.contains("int"), "{}", fields[1].1);
    }
}
