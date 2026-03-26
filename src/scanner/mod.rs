//! Spring MVC / Boot 源码扫描（tree-sitter-java）。

mod class_fields;
mod javadoc;
mod lazy_body;
mod parse;
mod params;
mod source_index;
mod spring;
mod swagger_openapi;

pub use lazy_body::ensure_request_body_resolved;
pub use params::BodyTemplatePolicy;
pub use source_index::JavaSourceCache;

use std::fs;
use std::path::{Component, Path, PathBuf};

use rayon::prelude::*;
use tree_sitter::{Parser, Tree};
use walkdir::WalkDir;

use crate::model::LocalApi;

use parse::{modifier_annotations, modifiers_node};
use params::extract_method_parameters;
use class_fields::{collect_field_catalog, FieldCatalog};
use spring::{
    class_level_request_mapping_path, is_controller_class, join_spring_paths,
    mapping_from_method_annotations, source_might_contain_spring_web_controller,
};

/// 扫描结果：接口列表 + 单文件解析错误（不中断整体）。
#[derive(Debug, Default)]
pub struct ScanReport {
    pub apis: Vec<LocalApi>,
    pub file_errors: Vec<String>,
}

/// 扫描策略：`Full` 启动即填好 `@RequestBody` 模板；`LazyEndpoints` 仅扫接口，DTO 延后到选中再解析（TUI 默认）。
#[derive(Debug, Clone, Copy, Default)]
pub enum ScanMode {
    /// 每文件解析一次并合并全局字段表，启动时生成全部 `body`（CLI `scan`、单测）。
    #[default]
    Full,
    /// 只解析疑似 Controller 的 `.java`，不解析纯 DTO 文件；配合 [`JavaSourceCache`] 与 [`ensure_request_body_resolved`]。
    LazyEndpoints,
}

/// 扫描 `root` 下所有 `.java` 文件（跳过 `target/`、`.git/`、`build/`、`node_modules/`）。默认 [`ScanMode::Full`]。
pub fn scan_project(root: &Path) -> ScanReport {
    scan_project_with_mode(root, ScanMode::Full)
}

pub fn scan_project_with_mode(root: &Path, mode: ScanMode) -> ScanReport {
    let root = match root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return ScanReport {
                apis: Vec::new(),
                file_errors: vec![format!("无法解析项目目录 {}: {e}", root.display())],
            };
        }
    };

    let mut report = ScanReport::default();
    let paths = collect_java_paths(&root);

    match mode {
        ScanMode::Full => scan_project_full(&root, &paths, &mut report),
        ScanMode::LazyEndpoints => scan_project_lazy_endpoints(&root, &paths, &mut report),
    }

    report.apis.sort_by(|a, b| {
        a.project_bucket
            .cmp(&b.project_bucket)
            .then_with(|| a.source_file.cmp(&b.source_file))
            .then_with(|| a.line.cmp(&b.line))
    });
    report
}

fn collect_java_paths(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "java"))
        .filter(|e| !should_skip_under_root(e.path(), root))
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn scan_project_full(root: &Path, paths: &[PathBuf], report: &mut ScanReport) {
    let parsed: Vec<Result<ParsedJavaFile, String>> = if paths.is_empty() {
        Vec::new()
    } else {
        paths
            .par_iter()
            .map(|path| parse_java_source_once(path))
            .collect()
    };

    let mut global_catalog = FieldCatalog::new();
    let mut controller_sources: Vec<ParsedJavaFile> = Vec::new();
    for res in parsed {
        match res {
            Ok(pf) => {
                merge_field_catalog(&mut global_catalog, &pf);
                if source_might_contain_spring_web_controller(&pf.source) {
                    controller_sources.push(pf);
                }
            }
            Err(msg) => report.file_errors.push(msg),
        }
    }

    if !controller_sources.is_empty() {
        let api_chunks: Vec<Vec<LocalApi>> = controller_sources
            .par_iter()
            .map(|pf| {
                collect_apis_from_parsed(pf, root, &global_catalog, BodyTemplatePolicy::Eager)
            })
            .collect();
        for mut v in api_chunks {
            report.apis.append(&mut v);
        }
    }
}

fn scan_project_lazy_endpoints(root: &Path, paths: &[PathBuf], report: &mut ScanReport) {
    let parsed: Vec<Result<Option<ParsedJavaFile>, String>> = if paths.is_empty() {
        Vec::new()
    } else {
        paths
            .par_iter()
            .map(|path| {
                let source =
                    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
                if !source_might_contain_spring_web_controller(&source) {
                    return Ok(None);
                }
                let mut parser =
                    java_parser().map_err(|e| format!("{}: 初始化解析器: {e}", path.display()))?;
                let tree = parser
                    .parse(&source, None)
                    .ok_or_else(|| format!("{}: 解析返回空树", path.display()))?;
                Ok(Some(ParsedJavaFile {
                    path: path.to_path_buf(),
                    source,
                    tree,
                }))
            })
            .collect()
    };

    let empty = FieldCatalog::new();
    let mut controller_sources: Vec<ParsedJavaFile> = Vec::new();
    for res in parsed {
        match res {
            Ok(Some(pf)) => controller_sources.push(pf),
            Ok(None) => {}
            Err(msg) => report.file_errors.push(msg),
        }
    }

    if !controller_sources.is_empty() {
        let api_chunks: Vec<Vec<LocalApi>> = controller_sources
            .par_iter()
            .map(|pf| collect_apis_from_parsed(pf, root, &empty, BodyTemplatePolicy::Deferred))
            .collect();
        for mut v in api_chunks {
            report.apis.append(&mut v);
        }
    }
}

/// `rel_path` 相对扫描根的第一级路径分量，用于多子项目目录分组（如 `order-api/src/...` → `order-api`）。
fn project_bucket_from_rel(rel_path: &Path) -> String {
    match rel_path.components().next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        Some(Component::CurDir) | None => ".".to_string(),
        _ => ".".to_string(),
    }
}

pub(crate) fn should_skip_under_root(path: &Path, root: &Path) -> bool {
    let rel = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return false,
    };
    rel.components().any(|c| {
        let s = c.as_os_str();
        s == "target" || s == ".git" || s == "build" || s == "node_modules"
    })
}

fn java_parser() -> anyhow::Result<Parser> {
    let mut p = Parser::new();
    p.set_language(&tree_sitter_java::LANGUAGE.into())?;
    Ok(p)
}

/// 单次磁盘读取 + tree-sitter 解析，供字段表与接口扫描复用。
struct ParsedJavaFile {
    path: PathBuf,
    source: String,
    tree: Tree,
}

fn parse_java_source_once(path: &Path) -> Result<ParsedJavaFile, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut parser = java_parser().map_err(|e| format!("{}: 初始化解析器: {e}", path.display()))?;
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| format!("{}: 解析返回空树", path.display()))?;
    Ok(ParsedJavaFile {
        path: path.to_path_buf(),
        source,
        tree,
    })
}

fn merge_field_catalog(global: &mut FieldCatalog, pf: &ParsedJavaFile) {
    for (k, v) in collect_field_catalog(pf.tree.root_node(), pf.source.as_bytes()) {
        global.insert(k, v);
    }
}

fn collect_apis_from_parsed(
    pf: &ParsedJavaFile,
    project_root: &Path,
    catalog: &FieldCatalog,
    body_policy: BodyTemplatePolicy,
) -> Vec<LocalApi> {
    let rel = pf
        .path
        .strip_prefix(project_root)
        .unwrap_or(&pf.path)
        .to_path_buf();
    let mut apis = Vec::new();
    walk_collect(
        pf.tree.root_node(),
        pf.source.as_bytes(),
        &rel,
        &mut apis,
        catalog,
        body_policy,
    );
    apis
}

fn walk_collect(
    node: tree_sitter::Node,
    source: &[u8],
    rel_path: &Path,
    apis: &mut Vec<LocalApi>,
    catalog: &class_fields::FieldCatalog,
    body_policy: BodyTemplatePolicy,
) {
    if node.kind() == "class_declaration" {
        process_class_declaration(node, source, rel_path, apis, catalog, body_policy);
    }
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            walk_collect(c, source, rel_path, apis, catalog, body_policy);
        }
    }
}

/// 扫描单个 Controller 类下各映射方法时的共享上下文。
struct ControllerClassCtx<'a> {
    class_name: &'a str,
    class_path_prefix: &'a str,
    class_body_start_byte: usize,
    class_doc: Option<String>,
    openapi_tag: Option<String>,
}

fn process_class_declaration(
    class: tree_sitter::Node,
    source: &[u8],
    rel_path: &Path,
    apis: &mut Vec<LocalApi>,
    catalog: &class_fields::FieldCatalog,
    body_policy: BodyTemplatePolicy,
) {
    let Some(class_name) = class_name(class, source) else {
        return;
    };
    let mods = modifiers_node(class);
    let class_anns = mods
        .map(|m| modifier_annotations(m))
        .unwrap_or_default();
    if !is_controller_class(&class_anns, source) {
        return;
    }

    let class_path_prefix = class_level_request_mapping_path(&class_anns, source);
    let class_doc = std::str::from_utf8(source)
        .ok()
        .and_then(|s| javadoc::method_javadoc_summary(s, class.start_byte()));
    let openapi_tag = swagger_openapi::class_tag_name(&class_anns, source);

    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let ctx = ControllerClassCtx {
        class_name: class_name.as_str(),
        class_path_prefix: class_path_prefix.as_str(),
        class_body_start_byte: body.start_byte(),
        class_doc,
        openapi_tag,
    };

    for i in 0..body.child_count() {
        let Some(child) = body.child(i) else { continue };
        if child.kind() != "method_declaration" {
            continue;
        }
        process_controller_method(child, source, rel_path, &ctx, apis, catalog, body_policy);
    }
}

fn class_name(class: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let id = class.child_by_field_name("name")?;
    id.utf8_text(source).ok().map(|s| s.to_string())
}

fn process_controller_method(
    method: tree_sitter::Node,
    source: &[u8],
    rel_path: &Path,
    class: &ControllerClassCtx<'_>,
    apis: &mut Vec<LocalApi>,
    catalog: &class_fields::FieldCatalog,
    body_policy: BodyTemplatePolicy,
) {
    let mods = modifiers_node(method);
    let method_anns = mods
        .map(|m| modifier_annotations(m))
        .unwrap_or_default();
    let Some(mapping) = mapping_from_method_annotations(&method_anns, source) else {
        return;
    };

    let method_name = method
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("unknown");
    let line = method.start_position().row as u32 + 1;
    let full_path = join_spring_paths(class.class_path_prefix, &mapping.path_suffix);

    let name = format!("{}.{}", class.class_name, method_name);
    let id = format!("{}:{}:{}", rel_path.display(), line, name);

    let ex = extract_method_parameters(method, source, catalog, body_policy);
    let bucket = project_bucket_from_rel(rel_path);
    let mut api = LocalApi::new_stub(
        id,
        name,
        class.class_name.to_string(),
        rel_path.display().to_string(),
        line,
        mapping.http_method,
        full_path,
        bucket,
    );
    api.path_params = ex.path_params;
    api.query_params = ex.query_params;
    api.headers = ex.headers;
    api.cookie_params = ex.cookie_params;
    api.model_params = ex.model_params;
    api.body_binding = ex.body_binding;
    api.body = ex.body;
    api.class_doc = class.class_doc.clone();
    api.openapi_tag = class.openapi_tag.clone();

    let jdoc = std::str::from_utf8(source).ok().and_then(|s| {
        javadoc::method_javadoc_summary_in_range(s, method.start_byte(), class.class_body_start_byte)
    });
    let op_sum = swagger_openapi::method_operation_summary(method, source);
    api.description = jdoc
        .filter(|d| !d.trim().is_empty())
        .or_else(|| op_sum.filter(|d| !d.trim().is_empty()));

    apis.push(api);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn project_bucket_first_segment() {
        assert_eq!(
            super::project_bucket_from_rel(Path::new("billing-svc/src/main/java/X.java")),
            "billing-svc"
        );
        assert_eq!(
            super::project_bucket_from_rel(Path::new("src/main/java/X.java")),
            "src"
        );
    }

    #[test]
    fn fixture_demo_controller() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spring_mvc");
        let r = scan_project(&root);
        assert!(
            r.file_errors.is_empty(),
            "errors: {:?}",
            r.file_errors
        );
        let paths: Vec<_> = r
            .apis
            .iter()
            .map(|a| (a.http_method.as_str(), a.path.as_str()))
            .collect();
        assert!(paths.contains(&("GET", "/api/v1/demo/hello/{id}")));
        assert!(paths.contains(&("POST", "/api/v1/demo/echo")));

        let get = r
            .apis
            .iter()
            .find(|a| a.path == "/api/v1/demo/hello/{id}")
            .expect("GET hello");
        assert_eq!(get.path_params.len(), 1);
        assert_eq!(get.path_params[0].name, "id");
        assert_eq!(get.query_params.len(), 1);
        assert_eq!(get.query_params[0].name, "q");
        assert_eq!(
            get.query_params[0].default_value.as_deref(),
            Some("all")
        );
        assert!(!get.query_params[0].required);

        let post = r
            .apis
            .iter()
            .find(|a| a.path == "/api/v1/demo/echo")
            .expect("POST echo");
        assert!(post.body.is_some());
        assert_eq!(
            post.body.as_ref().unwrap(),
            &serde_json::json!({"text":""})
        );
        assert_eq!(post.body_binding.as_ref().unwrap().name, "body");
        assert_eq!(post.body_binding.as_ref().unwrap().java_type, "EchoBody");

        let hello = r.apis.iter().find(|a| a.path == "/api/v1/demo/hello/{id}").unwrap();
        assert_eq!(hello.project_bucket, "src");
        assert!(hello.description.as_deref().unwrap_or("").contains("问候"));

        let echo = r.apis.iter().find(|a| a.path == "/api/v1/demo/echo").unwrap();
        assert!(echo.description.as_deref().unwrap_or("").contains("JSON"));
    }

    #[test]
    fn fixture_request_body_dto_in_separate_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/body_cross_file");
        let r = scan_project(&root);
        assert!(r.file_errors.is_empty(), "{:?}", r.file_errors);
        let post = r
            .apis
            .iter()
            .find(|a| a.path == "/api/cross/msg")
            .expect("POST /msg");
        assert!(post.body.is_some());
        assert_eq!(
            post.body.as_ref().unwrap(),
            &serde_json::json!({ "title": "", "priority": 0 })
        );
        assert_eq!(post.body_binding.as_ref().unwrap().java_type, "MessageDto");
    }

    #[test]
    fn lazy_endpoints_then_resolve_request_body() {
        use super::{ensure_request_body_resolved, JavaSourceCache};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/body_cross_file");
        let mut r = scan_project_with_mode(&root, ScanMode::LazyEndpoints);
        assert!(r.file_errors.is_empty(), "{:?}", r.file_errors);
        let post = r
            .apis
            .iter_mut()
            .find(|a| a.path == "/api/cross/msg")
            .expect("POST /msg");
        assert!(post.body.is_none());
        assert_eq!(post.body_binding.as_ref().unwrap().java_type, "MessageDto");
        let cache = JavaSourceCache::build(&root);
        assert!(ensure_request_body_resolved(post, &cache, &root));
        assert_eq!(
            post.body.as_ref().unwrap(),
            &serde_json::json!({ "title": "", "priority": 0 })
        );
    }

    #[test]
    fn fixture_swagger_operation_and_class_tag() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swagger_style");
        let r = scan_project(&root);
        assert!(r.file_errors.is_empty(), "{:?}", r.file_errors);
        let list = r
            .apis
            .iter()
            .find(|a| a.path == "/api/v2/finance/payment_bill/list")
            .expect("list endpoint");
        assert_eq!(list.description.as_deref(), Some("付款单列表"));
        assert_eq!(list.openapi_tag.as_deref(), Some("付款单管理"));
        assert!(list.class_doc.as_deref().unwrap_or("").contains("付款单"));
    }
}
