//! 本地 API 模型（与 PRD `LocalApi` 对齐）。

use serde::Serialize;

/// 扫描得到的单条 HTTP 接口。
#[derive(Debug, Clone, Serialize)]
pub struct LocalApi {
    pub id: String,
    /// 展示用名称，一般为 `ClassName.methodName`。
    pub name: String,
    /// 控制器 / 类名。
    pub module: String,
    /// 相对项目根的路径。
    pub source_file: String,
    /// 1-based 行号（映射注解所在方法起始行）。
    pub line: u32,
    pub http_method: String,
    /// 合并类级与方法级 path 后的完整路径（以 `/` 开头）。
    pub path: String,
    /// 扫描根目录下，`source_file` 相对路径的**第一级**目录名（多子项目工作区时常为子项目名，如 `order-service`）。
    #[serde(skip_serializing_if = "is_dot_bucket")]
    pub project_bucket: String,
    /// 方法说明：优先方法 Javadoc 首段，否则 `@Operation` / `@ApiOperation` 等。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类上 Javadoc 首段（如 `/** 付款单 */`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_doc: Option<String>,
    /// 类上 `@Tag(name = "...")`（OpenAPI 分组名）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openapi_tag: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<ApiParam>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<ApiParam>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<ApiParam>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cookie_params: Vec<ApiParam>,
    /// `@ModelAttribute` 等表单模型参数。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub model_params: Vec<ApiParam>,
    /// `@RequestBody` 形参信息（FR-03）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_binding: Option<ApiParam>,
    /// `@RequestBody` 推断的 JSON 模板（FR-04）。TUI 懒加载扫描下可为 `None`，选中接口后再补全。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiParam {
    pub name: String,
    pub java_type: String,
    pub location: ParamLocation,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamLocation {
    Path,
    Query,
    Header,
    Body,
    Cookie,
    Model,
}

impl LocalApi {
    /// 分组列表类标题：优先类注释 / OpenAPI 标签中含中日韩表意文字的文案，否则为 `module`（Java 类名）。
    pub fn module_group_label(&self) -> String {
        first_cjk_preferred_label(
            self.class_doc
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            self.openapi_tag
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            &self.module,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_stub(
        id: String,
        name: String,
        module: String,
        source_file: String,
        line: u32,
        http_method: String,
        path: String,
        project_bucket: String,
    ) -> Self {
        Self {
            id,
            name,
            module,
            source_file,
            line,
            http_method,
            path,
            project_bucket,
            description: None,
            class_doc: None,
            openapi_tag: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            headers: Vec::new(),
            cookie_params: Vec::new(),
            model_params: Vec::new(),
            body_binding: None,
            body: None,
            metadata: serde_json::Map::new(),
        }
    }

    /// 用于持久化请求草稿的稳定 key；避免源码行号变化导致草稿失效。
    pub fn request_draft_key(&self) -> String {
        let sig = format!(
            "v2\n{}\n{}\n{}\n{}\n{}",
            self.http_method, self.path, self.module, self.name, self.source_file
        );
        format!("v2:{:016x}", fnv1a64(sig.as_bytes()))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[allow(clippy::ptr_arg)]
fn is_dot_bucket(s: &String) -> bool {
    s.is_empty() || s.as_str() == "."
}

fn first_cjk_preferred_label(
    class_doc: Option<&str>,
    openapi_tag: Option<&str>,
    module: &str,
) -> String {
    if let Some(s) = class_doc {
        if contains_cjk(s) {
            return s.to_string();
        }
    }
    if let Some(s) = openapi_tag {
        if contains_cjk(s) {
            return s.to_string();
        }
    }
    module.to_string()
}

/// 是否含 CJK 统一表意文字（用于判断能否用中文类标题替代 Java 类名）。
fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x4E00..=0x9FFF).contains(&u)
            || (0x3400..=0x4DBF).contains(&u)
            || (0xF900..=0xFAFF).contains(&u)
            || (0x20000..=0x3134F).contains(&u)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_group_label_prefers_cjk_class_doc() {
        let mut a = LocalApi::new_stub(
            "1".into(),
            "X.m".into(),
            "FinPaymentBillController".into(),
            "f.java".into(),
            1,
            "POST".into(),
            "/p".into(),
            ".".into(),
        );
        a.class_doc = Some("付款单".into());
        a.openapi_tag = Some("Payment API".into());
        assert_eq!(a.module_group_label(), "付款单");
    }

    #[test]
    fn module_group_tag_when_doc_not_cjk() {
        let mut a = LocalApi::new_stub(
            "1".into(),
            "X.m".into(),
            "DemoController".into(),
            "f.java".into(),
            1,
            "GET".into(),
            "/p".into(),
            ".".into(),
        );
        a.class_doc = Some("Payment bill service".into());
        a.openapi_tag = Some("付款单管理".into());
        assert_eq!(a.module_group_label(), "付款单管理");
    }

    #[test]
    fn module_group_falls_back_to_java_name() {
        let mut a = LocalApi::new_stub(
            "1".into(),
            "X.m".into(),
            "DemoController".into(),
            "f.java".into(),
            1,
            "GET".into(),
            "/p".into(),
            ".".into(),
        );
        a.class_doc = Some("English only".into());
        a.openapi_tag = Some("UserResource".into());
        assert_eq!(a.module_group_label(), "DemoController");
    }
}
