//! tree-sitter Java 节点辅助函数。

use tree_sitter::Node;

/// 从 `@Foo` / `@pkg.Foo` 取简单类名 `Foo`。
pub fn annotation_simple_name(node: Node, source: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    simple_type_name(name_node, source)
}

fn simple_type_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "_reserved_identifier" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "scoped_identifier" => {
            let leaf = node.child_by_field_name("name")?;
            simple_type_name(leaf, source)
        }
        _ => None,
    }
}

/// 深度优先：子树中第一个 `string_literal` 的文本（不含引号）。
pub fn first_string_literal(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() == "string_literal" {
        return unquote_string(node.utf8_text(source).ok()?);
    }
    for i in 0..node.child_count() {
        let child = node.child(i)?;
        if let Some(s) = first_string_literal(child, source) {
            return Some(s);
        }
    }
    None
}

fn unquote_string(raw: &str) -> Option<String> {
    let t = raw.trim();
    let t = t.strip_prefix('"')?.strip_suffix('"')?;
    Some(
        t.replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\t", "\t"),
    )
}

pub fn modifiers_node(class_or_method: Node) -> Option<Node> {
    let c = class_or_method.child(0)?;
    if c.kind() == "modifiers" {
        Some(c)
    } else {
        None
    }
}

/// 收集 `modifiers` 下全部注解节点。
pub fn modifier_annotations(modifiers: Node) -> Vec<Node> {
    let mut out = Vec::new();
    for i in 0..modifiers.child_count() {
        if let Some(ch) = modifiers.child(i) {
            if ch.kind() == "annotation" || ch.kind() == "marker_annotation" {
                out.push(ch);
            }
        }
    }
    out
}

/// 注解命名参数 `key = "..."`。
pub fn annotation_string_attr(ann: Node, source: &[u8], key: &str) -> Option<String> {
    if ann.kind() == "marker_annotation" {
        return None;
    }
    let args = ann.child_by_field_name("arguments")?;
    for i in 0..args.named_child_count() {
        let child = args.named_child(i)?;
        if child.kind() != "element_value_pair" {
            continue;
        }
        let k = child.child_by_field_name("key")?;
        if k.utf8_text(source).ok()? != key {
            continue;
        }
        let value = child.child_by_field_name("value")?;
        return first_string_literal(value, source);
    }
    None
}

/// 命名参数 `key = true|false`。
pub fn annotation_bool_attr(ann: Node, source: &[u8], key: &str) -> Option<bool> {
    if ann.kind() == "marker_annotation" {
        return None;
    }
    let args = ann.child_by_field_name("arguments")?;
    for i in 0..args.named_child_count() {
        let child = args.named_child(i)?;
        if child.kind() != "element_value_pair" {
            continue;
        }
        let k = child.child_by_field_name("key")?;
        if k.utf8_text(source).ok()? != key {
            continue;
        }
        let value = child.child_by_field_name("value")?;
        return Some(match value.kind() {
            "true" => true,
            "false" => false,
            _ => match value.utf8_text(source).ok()? {
                "true" => true,
                "false" => false,
                _ => return None,
            },
        });
    }
    None
}

/// `value` / `name`，或唯一位置参数 `@RequestParam("page")`。
pub fn annotation_binding_name(ann: Node, source: &[u8], fallback: &str) -> String {
    for key in ["value", "name"] {
        if let Some(s) = annotation_string_attr(ann, source, key) {
            if !s.is_empty() {
                return s;
            }
        }
    }
    if let Some(s) = annotation_positional_single_string(ann, source) {
        if !s.is_empty() {
            return s;
        }
    }
    fallback.to_string()
}

/// `@Ann("x")`：参数列表仅一个非 `name=` 元素时的字符串。
fn annotation_positional_single_string(ann: Node, source: &[u8]) -> Option<String> {
    if ann.kind() == "marker_annotation" {
        return None;
    }
    let args = ann.child_by_field_name("arguments")?;
    if args.named_child_count() != 1 {
        return None;
    }
    let c = args.named_child(0)?;
    if c.kind() == "element_value_pair" {
        return None;
    }
    first_string_literal(c, source)
}

pub fn node_text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}
