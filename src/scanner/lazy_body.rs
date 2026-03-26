//! 选中接口时再按需解析 `@RequestBody` 类型依赖的源码，合并 `FieldCatalog` 后生成 JSON 模板。

use std::collections::{HashSet, VecDeque};
use std::path::Path;

use regex::Regex;
use std::sync::OnceLock;
use tree_sitter::Parser;

use crate::model::LocalApi;

use super::class_fields::{collect_field_catalog, FieldCatalog};
use super::params::request_body_json_template;
use super::JavaSourceCache;

/// 为 `api` 补全 `body`（若此前为懒加载且源码索引已就绪）。成功写入则返回 `true`。
pub fn ensure_request_body_resolved(
    api: &mut LocalApi,
    cache: &JavaSourceCache,
    _project_root: &Path,
) -> bool {
    if api.body.is_some() || api.body_binding.is_none() {
        return false;
    }
    let Some(bind) = api.body_binding.as_ref() else {
        return false;
    };
    let java_type = bind.java_type.as_str();
    let Some(catalog) = build_catalog_for_body_type(cache, java_type) else {
        return false;
    };
    let v = request_body_json_template(java_type, &catalog);
    api.body = Some(v);
    true
}

fn build_catalog_for_body_type(cache: &JavaSourceCache, java_type: &str) -> Option<FieldCatalog> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .ok()?;

    let seed = body_dto_seed_simple_name(java_type);
    let mut catalog = FieldCatalog::new();
    let mut visited_types: HashSet<String> = HashSet::new();
    let mut visited_files: HashSet<std::path::PathBuf> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(seed);

    while let Some(t) = queue.pop_front() {
        if !visited_types.insert(t.clone()) {
            continue;
        }
        for rel in cache.paths_for_simple_type(&t) {
            if !visited_files.insert(rel.clone()) {
                continue;
            }
            let Some(src) = cache.source_rel(rel) else {
                continue;
            };
            let new_keys = merge_parse_catalog(src.as_ref(), &mut catalog, &mut parser);
            for type_name in new_keys {
                if let Some(fields) = catalog.get(&type_name) {
                    for (_, fty) in fields {
                        for dep in referenced_dto_simple_names(fty) {
                            if !catalog.contains_key(&dep) {
                                queue.push_back(dep);
                            }
                        }
                    }
                }
            }
        }
    }

    Some(catalog)
}

fn merge_parse_catalog(
    source: &str,
    catalog: &mut FieldCatalog,
    parser: &mut Parser,
) -> HashSet<String> {
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };
    let file_cat = collect_field_catalog(tree.root_node(), source.as_bytes());
    let mut new_keys = HashSet::new();
    for (k, v) in file_cat {
        new_keys.insert(k.clone());
        catalog.insert(k, v);
    }
    new_keys
}

fn body_dto_seed_simple_name(java_type: &str) -> String {
    let mut s: String = java_type.chars().filter(|c| !c.is_whitespace()).collect();
    loop {
        let before = s.clone();
        if let Some(inner) = peel_one_wrapper(&s) {
            s = inner.to_string();
        }
        if s == before {
            break;
        }
    }
    outer_simple_class_name(&s)
}

fn peel_one_wrapper(t: &str) -> Option<&str> {
    strip_wrapped(t, "java.util.Optional<")
        .or_else(|| strip_wrapped(t, "Optional<"))
        .or_else(|| strip_wrapped(t, "java.util.List<"))
        .or_else(|| strip_wrapped(t, "List<"))
        .or_else(|| strip_wrapped(t, "java.util.Set<"))
        .or_else(|| strip_wrapped(t, "Set<"))
}

fn strip_wrapped<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
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

fn outer_simple_class_name(java_type: &str) -> String {
    let base = java_type.split('<').next().unwrap_or(java_type).trim();
    let seg = base.rsplit('.').next().unwrap_or(base);
    seg.split('$').next_back().unwrap_or(seg).to_string()
}

fn referenced_dto_simple_names(java_type: &str) -> Vec<String> {
    let re = dto_token_re();
    let mut out = Vec::new();
    for cap in re.captures_iter(java_type) {
        let s = cap[1].to_string();
        if is_java_type_noise(&s) {
            continue;
        }
        out.push(s);
    }
    out
}

fn dto_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([A-Z][\w]*)\b").expect("regex"))
}

fn is_java_type_noise(s: &str) -> bool {
    matches!(
        s,
        "String"
            | "Object"
            | "Class"
            | "Integer"
            | "Long"
            | "Short"
            | "Byte"
            | "Double"
            | "Float"
            | "Boolean"
            | "Character"
            | "Void"
    ) || s.len() <= 1
}
