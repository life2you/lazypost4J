//! 全项目 Java 源码纯文本索引（不跑 tree-sitter）：供懒加载 DTO 解析时按简单类名查文件。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

use regex::Regex;
use walkdir::WalkDir;

use super::should_skip_under_root;

/// 相对扫描根路径 → 源码文本（`Arc` 便于多线程读、少复制）。
#[derive(Debug)]
pub struct JavaSourceCache {
    files: HashMap<PathBuf, Arc<str>>,
    type_index: HashMap<String, Vec<PathBuf>>,
    pub load_errors: Vec<String>,
}

impl JavaSourceCache {
    /// 遍历与接口扫描相同的路径规则，读入全部 `.java` 文本；用正则抽取顶层 `class` / `record` 等简单名。
    pub fn build(root: &Path) -> Self {
        let root = match root.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return Self {
                    files: HashMap::new(),
                    type_index: HashMap::new(),
                    load_errors: vec![format!("无法解析项目目录 {}: {e}", root.display())],
                };
            }
        };

        let paths: Vec<PathBuf> = WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|x| x == "java"))
            .filter(|e| !should_skip_under_root(e.path(), &root))
            .map(|e| e.path().to_path_buf())
            .collect();

        let mut files = HashMap::new();
        let mut type_index: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut load_errors = Vec::new();

        for path in paths {
            let rel = match path.strip_prefix(&root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => path.clone(),
            };
            match std::fs::read_to_string(&path) {
                Ok(source) => {
                    index_declarations(&source, &rel, &mut type_index);
                    files.insert(rel, Arc::from(source.into_boxed_str()));
                }
                Err(e) => load_errors.push(format!("{}: {e}", path.display())),
            }
        }

        Self {
            files,
            type_index,
            load_errors,
        }
    }

    pub fn source_rel(&self, rel: &Path) -> Option<&Arc<str>> {
        self.files.get(rel)
    }

    /// 简单类名 → 可能定义该类型的相对路径列表（跨包重名时会有多项）。
    pub fn paths_for_simple_type(&self, simple: &str) -> &[PathBuf] {
        self.type_index
            .get(simple)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

fn type_decl_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*\s*(?:public|protected|private)?\s*(?:(?:abstract|static|final|sealed|non-sealed|strictfp)\s+){0,3}(class|interface|enum|record)\s+(\w+)\b",
        )
        .expect("regex")
    })
}

fn index_declarations(source: &str, rel: &Path, type_index: &mut HashMap<String, Vec<PathBuf>>) {
    let re = type_decl_regex();
    for cap in re.captures_iter(source) {
        let name = cap[2].to_string();
        type_index.entry(name).or_default().push(rel.to_path_buf());
    }
}
