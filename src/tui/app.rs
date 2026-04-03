//! TUI 应用状态。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::ListState;

use crate::http_exec::HttpResult;
use crate::model::LocalApi;
use crate::scanner::{JavaSourceCache, ScanReport};
use crate::user_config::{HeaderEntry, HostEntry, StoredRequestDraft};

/// 左侧列表分组方式（`g` 循环切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ListGroupMode {
    /// 按 Controller 类分组。
    ByController,
    /// 按扫描根下首级目录分组，其下再按类（默认进入时全部折叠）。
    #[default]
    ByProjectFolder,
    /// 不按组，平铺接口。
    Flat,
}

/// 左侧列表的一行：类标题（仅分组模式）或具体接口。
#[derive(Debug, Clone)]
pub enum ListRow {
    /// `scope_bucket`：`None` 表示「按类」顶层分组；`Some` 表示在「按项目」视图下该类所属子项目目录。
    ClassHeader {
        scope_bucket: Option<String>,
        module: String,
        label: String,
    },
    /// `bucket` 为 `LocalApi.project_bucket`（首级目录名）。
    ProjectHeader(String),
    Endpoint {
        api_index: usize,
    },
}

/// 域名面板内编辑 URL 或描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostsEditKind {
    Url,
    Description,
}

/// 请求头面板内编辑名称、值或描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestHeaderEditKind {
    Name,
    Value,
    Description,
}

/// 详情区按 HTTP 方法与是否带请求体给出的默认头（非 `a` 全局头、非注解扫描列表）。
/// 例如：有 Body 时加 `Content-Type: application/json`；GET/POST 等均带 `Accept: application/json`。
/// 主界面聚焦区块（数字键 1–7、鼠标左键切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainPanel {
    /// 接口列表。
    #[default]
    ApiList,
    /// 域名侧栏。
    HostSidebar,
    /// 全局请求头侧栏。
    HeaderSidebar,
    /// 详情。
    Detail,
    /// 响应区。
    Response,
}

/// 详情区内部聚焦的子 pane。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailPane {
    #[default]
    Params,
    Headers,
    Body,
}

#[derive(Debug, Clone, Default)]
struct ApiRequestDraft {
    path_vals: HashMap<String, String>,
    query_vals: HashMap<String, String>,
    extra_query_params: Vec<(String, String)>,
    method_detail_headers: Vec<(String, String)>,
    extra_request_headers: Vec<(String, String)>,
    header_vals: HashMap<String, String>,
    cookie_vals: HashMap<String, String>,
    body_draft: String,
}

#[derive(Debug, Clone)]
pub struct ResponseJsonLine {
    pub depth: usize,
    pub path: String,
    pub label: String,
    pub expandable: bool,
    pub expanded: bool,
}

impl From<StoredRequestDraft> for ApiRequestDraft {
    fn from(value: StoredRequestDraft) -> Self {
        Self {
            path_vals: value.path_vals,
            query_vals: value.query_vals,
            extra_query_params: value.extra_query_params,
            method_detail_headers: value.method_detail_headers,
            extra_request_headers: value.extra_request_headers,
            header_vals: value.header_vals,
            cookie_vals: value.cookie_vals,
            body_draft: value.body_draft,
        }
    }
}

impl From<ApiRequestDraft> for StoredRequestDraft {
    fn from(value: ApiRequestDraft) -> Self {
        Self {
            path_vals: value.path_vals,
            query_vals: value.query_vals,
            extra_query_params: value.extra_query_params,
            method_detail_headers: value.method_detail_headers,
            extra_request_headers: value.extra_request_headers,
            header_vals: value.header_vals,
            cookie_vals: value.cookie_vals,
            body_draft: value.body_draft,
        }
    }
}

/// 上一帧主界面几何（鼠标命中、滚轮可视高度）；`api_list_inner` 用于列表行换算。
#[derive(Debug, Clone, Copy)]
pub struct MainUiLayout {
    pub api_list: Rect,
    pub api_list_inner: Rect,
    pub host_sidebar: Rect,
    pub host_sidebar_inner: Rect,
    pub header_sidebar: Rect,
    pub header_sidebar_inner: Rect,
    pub detail: Rect,
    pub detail_summary: Rect,
    pub detail_params: Rect,
    pub detail_params_inner: Rect,
    pub detail_headers: Rect,
    pub detail_headers_inner: Rect,
    pub detail_body: Rect,
    pub detail_body_inner: Rect,
    pub response: Rect,
    pub response_inner: Rect,
}

fn default_method_detail_headers(
    _http_method: &str,
    has_request_body: bool,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if has_request_body {
        out.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    out.push(("Accept".to_string(), "application/json".to_string()));
    out
}

pub struct App {
    pub project: PathBuf,
    /// 可切换的服务根（含可选描述）。
    pub hosts: Vec<HostEntry>,
    /// `hosts` 中下标：发请求使用的项。
    pub base_url_index: usize,
    /// 发送时附加的 HTTP 头（`name` 非空才会加入请求）。
    pub request_headers: Vec<HeaderEntry>,
    /// `request_headers` 中下标：侧栏 ▶ 与底栏摘要。
    pub selected_request_header: usize,
    pub apis: Vec<LocalApi>,
    pub filtered: Vec<usize>,
    pub filter: String,
    pub list_group_mode: ListGroupMode,
    /// 按类分组时已折叠的 Controller `module` 键。
    pub collapsed_modules: HashSet<String>,
    /// 按目录分组时已折叠的 `project_bucket` 键。
    pub collapsed_project_buckets: HashSet<String>,
    /// 「按项目」视图下，已折叠的 (项目目录, Controller 类名)。
    pub collapsed_class_in_project: HashSet<(String, String)>,
    pending_collapse_all_modules: bool,
    pending_collapse_all_projects: bool,
    /// 首次「按项目」列表重建时，把所有 (项目, Controller) 类组设为折叠。
    pending_collapse_all_classes_in_projects: bool,
    pub list_rows: Vec<ListRow>,
    pub list_state: ListState,
    pub scan_errors: Vec<String>,
    /// 当前列表选中对应的接口（列表变化时刷新）。
    pub current_api: Option<LocalApi>,
    pub path_vals: HashMap<String, String>,
    pub query_vals: HashMap<String, String>,
    /// 手工附加的 Query（非扫描）；参与拼接 URL；切换接口时清空。
    pub extra_query_params: Vec<(String, String)>,
    /// 详情 / `e` 表单：按当前方法（GET/POST…）与是否有 Body 预设的请求头，可改值（顺序即发送时相对顺序，不含全局 `a`）。
    pub method_detail_headers: Vec<(String, String)>,
    /// 当前请求临时附加的 Header，仅当前接口有效。
    pub extra_request_headers: Vec<(String, String)>,
    /// 扫描到的 `@RequestHeader` 当前值：在详情与 `e` 表单里可编辑，并参与发送合并。
    pub header_vals: HashMap<String, String>,
    /// 扫描到的 `@CookieValue` 当前值：在详情与 `e` 表单里可编辑，并参与发送。
    pub cookie_vals: HashMap<String, String>,
    pub body_draft: String,
    request_drafts: HashMap<String, ApiRequestDraft>,
    pub last_http: Option<HttpResult>,
    pub response_json: Option<serde_json::Value>,
    response_json_expanded: HashSet<String>,
    pub response_focus: usize,
    pub pending_request: bool,
    pub status_msg: String,
    pub search_focus: bool,
    pub response_view_open: bool,
    pub response_view_scroll: u16,
    pub response_view_max_scroll: u16,
    /// 域名列表面板（按 h）。
    pub hosts_panel_open: bool,
    /// 在面板内编辑某一行的缓冲。
    pub hosts_edit_line: bool,
    /// 编辑行时：改 URL 还是描述。
    pub hosts_edit_kind: Option<HostsEditKind>,
    pub host_cursor_char: usize,
    pub hosts_list_state: ListState,
    pub host_buf: String,
    /// 请求头列表面板（按 a）。
    pub headers_panel_open: bool,
    pub headers_edit_line: bool,
    pub headers_edit_kind: Option<RequestHeaderEditKind>,
    pub header_cursor_char: usize,
    pub headers_list_state: ListState,
    pub header_buf: String,
    pub body_buf: String,
    /// Body 多行编辑：插入点为「字符下标」（0 = 文首，最大为 `chars().count()`）。
    pub body_cursor_char: usize,
    /// Path / Query / 方法预设请求头 合一扁平下标；`None` 表示尚未用 `[`/`]` 选中。
    pub param_focus: Option<usize>,
    /// Path/Query 单行值（在「请求表单」下方编辑区使用）。
    pub param_edit_buf: String,
    /// Path/Query 单行编辑：插入点字符下标。
    pub param_cursor_char: usize,
    pub show_help: bool,
    /// 后台扫描线程尚未返回结果（用于 `r` 防抖与首屏提示）。
    pub scan_in_flight: bool,
    /// 全量 Java 源码内存索引（无 AST）；后台加载，供懒解析 `@RequestBody`。
    pub source_cache: Option<Arc<JavaSourceCache>>,
    /// 源码索引是否仍在后台构建。
    pub source_cache_loading: bool,
    /// Postman 式请求表单：一行一类参数 + Body，集中预览与编辑。
    pub request_editor_open: bool,
    /// 表单内扁平焦点：先全部 Path，再扫描 Query，再附加 Query，再预设头，Body（若有）。
    pub request_editor_focus: usize,
    /// 焦点在「附加 Query」行时：`true` 编辑参数名，`false` 编辑值。
    pub request_editor_extra_kv: bool,
    /// 请求编辑弹窗内的即时错误提示，例如 Body JSON 校验失败。
    pub request_editor_error: Option<String>,
    /// 主界面当前焦点区块（1–6 / 鼠标切换）。
    pub main_panel: MainPanel,
    /// 详情区当前子 pane 焦点。
    pub detail_pane: DetailPane,
    /// 最近一次绘制时的主界面布局（供鼠标命中）。
    pub last_main_layout: Option<MainUiLayout>,
    pub detail_params_scroll: u16,
    pub detail_headers_scroll: u16,
    pub detail_body_scroll: u16,
    /// 响应区 Paragraph 垂直滚动行数。
    pub response_scroll: u16,
    pub host_sidebar_scroll: u16,
    pub header_sidebar_scroll: u16,
}

impl App {
    pub fn new(project: PathBuf) -> Self {
        let cfg = crate::user_config::load()
            .unwrap_or_else(|_| crate::user_config::UserConfig::default());
        let project_key = std::fs::canonicalize(&project)
            .unwrap_or_else(|_| project.clone())
            .display()
            .to_string();
        let mut hosts = if cfg.hosts.is_empty() {
            crate::user_config::UserConfig::default().hosts
        } else {
            cfg.hosts
        };
        for h in &mut hosts {
            if h.url.trim().is_empty() {
                h.url = "http://localhost:8080".to_string();
            }
        }
        let base_url_index = cfg.selected_base_url.min(hosts.len().saturating_sub(1));
        let request_headers = cfg.request_headers;
        let selected_request_header = if request_headers.is_empty() {
            0
        } else {
            cfg.selected_request_header.min(request_headers.len() - 1)
        };
        let request_drafts = cfg
            .request_drafts_by_project
            .get(&project_key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(api_id, draft)| (api_id, ApiRequestDraft::from(draft)))
            .collect();

        Self {
            project,
            hosts,
            base_url_index,
            request_headers,
            selected_request_header,
            apis: Vec::new(),
            filtered: Vec::new(),
            filter: String::new(),
            list_group_mode: ListGroupMode::ByProjectFolder,
            collapsed_modules: HashSet::new(),
            collapsed_project_buckets: HashSet::new(),
            collapsed_class_in_project: HashSet::new(),
            pending_collapse_all_modules: false,
            pending_collapse_all_projects: true,
            pending_collapse_all_classes_in_projects: true,
            list_rows: Vec::new(),
            list_state: ListState::default(),
            scan_errors: Vec::new(),
            current_api: None,
            path_vals: HashMap::new(),
            query_vals: HashMap::new(),
            extra_query_params: Vec::new(),
            method_detail_headers: Vec::new(),
            extra_request_headers: Vec::new(),
            header_vals: HashMap::new(),
            cookie_vals: HashMap::new(),
            body_draft: String::new(),
            request_drafts,
            last_http: None,
            response_json: None,
            response_json_expanded: HashSet::new(),
            response_focus: 0,
            pending_request: false,
            status_msg: String::new(),
            search_focus: false,
            response_view_open: false,
            response_view_scroll: 0,
            response_view_max_scroll: 0,
            hosts_panel_open: false,
            hosts_edit_line: false,
            hosts_edit_kind: None,
            host_cursor_char: 0,
            hosts_list_state: ListState::default(),
            host_buf: String::new(),
            headers_panel_open: false,
            headers_edit_line: false,
            headers_edit_kind: None,
            header_cursor_char: 0,
            headers_list_state: ListState::default(),
            header_buf: String::new(),
            body_buf: String::new(),
            body_cursor_char: 0,
            param_focus: None,
            param_edit_buf: String::new(),
            param_cursor_char: 0,
            show_help: false,
            scan_in_flight: false,
            source_cache: None,
            source_cache_loading: true,
            request_editor_open: false,
            request_editor_focus: 0,
            request_editor_extra_kv: false,
            request_editor_error: None,
            main_panel: MainPanel::default(),
            detail_pane: DetailPane::default(),
            last_main_layout: None,
            detail_params_scroll: 0,
            detail_headers_scroll: 0,
            detail_body_scroll: 0,
            response_scroll: 0,
            host_sidebar_scroll: 0,
            header_sidebar_scroll: 0,
        }
    }

    pub fn active_base_url(&self) -> &str {
        self.hosts
            .get(self.base_url_index)
            .map(|h| h.url.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("http://localhost:8080")
    }

    fn clear_transient_ui_for_module_switch(&mut self) {
        if self.request_editor_open {
            self.save_request_editor_row();
            self.request_editor_open = false;
            self.request_editor_error = None;
            self.persist_current_request_draft();
        }
        if self.hosts_edit_line {
            self.cancel_host_line_edit();
        }
        if self.headers_edit_line {
            self.cancel_header_line_edit();
        }
        self.hosts_panel_open = false;
        self.headers_panel_open = false;
        self.search_focus = false;
        self.response_view_open = false;
        self.response_view_scroll = 0;
        self.response_view_max_scroll = 0;
    }

    pub fn select_main_module_digit(&mut self, c: char) -> bool {
        self.clear_transient_ui_for_module_switch();
        match c {
            '1' => self.main_panel = MainPanel::ApiList,
            '2' => self.main_panel = MainPanel::HostSidebar,
            '3' => self.main_panel = MainPanel::HeaderSidebar,
            '4' => {
                self.main_panel = MainPanel::Detail;
                self.detail_pane = DetailPane::Params;
            }
            '5' => {
                self.main_panel = MainPanel::Detail;
                self.detail_pane = DetailPane::Headers;
            }
            '6' => {
                self.main_panel = MainPanel::Detail;
                self.detail_pane = DetailPane::Body;
            }
            '7' => self.main_panel = MainPanel::Response,
            _ => return false,
        }
        true
    }

    pub fn edit_current_module(&mut self) -> Result<(), &'static str> {
        match self.main_panel {
            MainPanel::ApiList => Err("接口列表不可直接编辑；先切到 2/3/4/5/6 对应模块按 e"),
            MainPanel::HostSidebar => {
                self.open_hosts_panel();
                self.begin_edit_host_url();
                Ok(())
            }
            MainPanel::HeaderSidebar => {
                self.open_headers_panel();
                if self.request_headers.is_empty() {
                    self.add_header_row();
                }
                self.begin_edit_header_value();
                Ok(())
            }
            MainPanel::Detail => {
                self.ensure_current_api_selected()?;
                self.open_request_editor_for_detail_pane()
            }
            MainPanel::Response => Err("响应区不可编辑"),
        }
    }

    pub fn ensure_current_api_selected(&mut self) -> Result<(), &'static str> {
        if self.current_api.is_some() {
            return Ok(());
        }
        if let Some(ix) = self.first_endpoint_row_index() {
            self.list_state.select(Some(ix));
            self.sync_detail_from_selection();
            if self.current_api.is_some() {
                return Ok(());
            }
        }
        match self.list_group_mode {
            ListGroupMode::ByController => self.collapsed_modules.clear(),
            ListGroupMode::ByProjectFolder => {
                self.collapsed_project_buckets.clear();
                self.collapsed_class_in_project.clear();
            }
            ListGroupMode::Flat => {}
        }
        self.rebuild_list_rows();
        if let Some(ix) = self.first_endpoint_row_index() {
            self.list_state.select(Some(ix));
            self.sync_detail_from_selection();
            if self.current_api.is_some() {
                self.status_msg = "已自动展开并选中首个接口".into();
                return Ok(());
            }
        }
        Err("请先在左侧展开并选择具体接口")
    }

    pub fn open_request_editor_for_detail_pane(&mut self) -> Result<(), &'static str> {
        if self.request_editor_open {
            self.save_request_editor_row();
        }
        self.request_editor_error = None;

        let Some(api) = self.current_api.clone() else {
            return Err("请先选择接口");
        };

        let params_count =
            api.path_params.len() + api.query_params.len() + self.extra_query_params.len();
        let headers_start = params_count;
        let headers_count = self.method_detail_headers.len()
            + self.extra_request_headers.len()
            + api.cookie_params.len()
            + api.headers.len();
        let total = self.request_sheet_row_count();

        match self.detail_pane {
            DetailPane::Params => {
                if params_count == 0 {
                    self.extra_query_params.push((String::new(), String::new()));
                    let pn = api.path_params.len();
                    let qn = api.query_params.len();
                    self.request_editor_focus = pn + qn;
                    self.request_editor_extra_kv = true;
                } else {
                    self.request_editor_focus = 0;
                    self.request_editor_extra_kv = false;
                }
            }
            DetailPane::Headers => {
                if headers_count == 0 {
                    self.extra_request_headers
                        .push(("X-New-Header".into(), String::new()));
                    self.request_editor_focus = headers_start;
                    self.request_editor_extra_kv = true;
                } else {
                    self.request_editor_focus = headers_start;
                    self.request_editor_extra_kv = false;
                }
            }
            DetailPane::Body => {
                if api.body_binding.is_none() {
                    return Err("当前接口无 Body 可编辑");
                }
                self.request_editor_focus = total.saturating_sub(1);
                self.request_editor_extra_kv = false;
            }
        }

        self.request_editor_open = true;
        self.load_request_editor_row();
        Ok(())
    }

    pub fn detail_summary_text(&self) -> String {
        let Some(api) = &self.current_api else {
            return "从左侧列表选择一个接口后，这里会显示接口摘要。".into();
        };
        let mut s = String::new();
        s.push_str(&format!("{}  {}\n", api.http_method, api.path));
        s.push_str(&format!("{}\n", api.name));
        if let Some(doc) = &api.description {
            if !doc.trim().is_empty() {
                s.push_str(&format!("{doc}\n"));
            }
        }
        s.push_str(&format!("文件: {}:{}\n", api.source_file, api.line));
        s.push_str(&format!("Base: {}\n", self.active_base_url()));
        match crate::http_exec::compose_url(
            self.active_base_url(),
            &api.path,
            &self.path_vals,
            &self.merged_query_for_url(),
        ) {
            Ok(u) => s.push_str(&format!("预览: {u}")),
            Err(e) => s.push_str(&format!("预览失败: {e}")),
        }
        s
    }

    pub fn detail_params_text(&self) -> String {
        let Some(api) = &self.current_api else {
            return "当前未选中接口。".into();
        };
        let mut s = String::new();
        if !api.path_params.is_empty() {
            s.push_str("[Path]\n");
            for p in &api.path_params {
                let v = self.path_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!("{} : {} = {:?}\n", p.name, p.java_type, v));
            }
            s.push('\n');
        }
        if !api.query_params.is_empty() {
            s.push_str("[Query]\n");
            for p in &api.query_params {
                let v = self.query_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!("{} : {} = {:?}\n", p.name, p.java_type, v));
            }
            s.push('\n');
        }
        if !self.extra_query_params.is_empty() {
            s.push_str("[附加 Query]\n");
            for (k, (ek, ev)) in self.extra_query_params.iter().enumerate() {
                let label = if ek.trim().is_empty() {
                    format!("(第 {} 行空参数名)", k + 1)
                } else {
                    ek.clone()
                };
                s.push_str(&format!("{} = {:?}\n", label, ev));
            }
            s.push('\n');
        }
        if !api.model_params.is_empty() {
            s.push_str("[ModelAttribute] （已扫描，发送闭环待实现）\n");
            for p in &api.model_params {
                s.push_str(&format!("{} : {}\n", p.name, p.java_type));
            }
            s.push('\n');
        }
        if s.trim().is_empty() {
            "当前接口没有 Path / Query / Model 参数。".into()
        } else {
            s
        }
    }

    pub fn detail_headers_text(&self) -> String {
        let Some(api) = &self.current_api else {
            return "当前未选中接口。".into();
        };
        let mut s = String::new();
        if !self.method_detail_headers.is_empty() {
            s.push_str("[预设 Header]\n");
            for (k, v) in &self.method_detail_headers {
                s.push_str(&format!("{k} = {:?}\n", v));
            }
            s.push('\n');
        }
        if !self.extra_request_headers.is_empty() {
            s.push_str("[附加 Header]\n");
            for (k, v) in &self.extra_request_headers {
                s.push_str(&format!("{k} = {:?}\n", v));
            }
            s.push('\n');
        }
        if !api.cookie_params.is_empty() {
            s.push_str("[Cookie]\n");
            for p in &api.cookie_params {
                let v = self.cookie_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!("{} : {} = {:?}\n", p.name, p.java_type, v));
            }
            s.push('\n');
        }
        if !api.headers.is_empty() {
            s.push_str("[@RequestHeader]\n");
            for p in &api.headers {
                let v = self.header_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!("{} : {} = {:?}\n", p.name, p.java_type, v));
            }
            s.push('\n');
        }
        if s.trim().is_empty() {
            "当前接口没有 Header / Cookie 参数。".into()
        } else {
            s
        }
    }

    pub fn detail_body_text(&self) -> String {
        let Some(api) = &self.current_api else {
            return "当前未选中接口。".into();
        };
        let Some(binding) = &api.body_binding else {
            return "当前接口无 @RequestBody。".into();
        };
        let mut s = String::new();
        s.push_str(&format!("{} : {}\n\n", binding.name, binding.java_type));
        if api.body.is_none() {
            s.push_str("(Body JSON 等待源码索引或无法解析 DTO，片刻后重试或检查类型名)");
        } else {
            s.push_str(&self.body_draft);
        }
        s
    }

    pub fn persist_config(&mut self) {
        let mut cfg = crate::user_config::load().unwrap_or_default();
        cfg.hosts = self.hosts.clone();
        cfg.selected_base_url = self.base_url_index;
        cfg.request_headers = self.request_headers.clone();
        cfg.selected_request_header = self.selected_request_header;
        let project_key = std::fs::canonicalize(&self.project)
            .unwrap_or_else(|_| self.project.clone())
            .display()
            .to_string();
        let drafts = self
            .request_drafts
            .iter()
            .map(|(api_id, draft)| (api_id.clone(), StoredRequestDraft::from(draft.clone())))
            .collect();
        cfg.request_drafts_by_project.insert(project_key, drafts);
        if let Err(e) = crate::user_config::save(&cfg) {
            self.status_msg = format!("保存配置失败: {e}");
        }
    }

    pub fn open_hosts_panel(&mut self) {
        self.hosts_panel_open = true;
        self.hosts_edit_line = false;
        self.hosts_edit_kind = None;
        let i = self.base_url_index.min(self.hosts.len().saturating_sub(1));
        self.hosts_list_state.select(Some(i));
    }

    pub fn add_host_row(&mut self) {
        self.hosts.push(HostEntry {
            url: "http://localhost:8080".into(),
            description: None,
        });
        let last = self.hosts.len() - 1;
        self.hosts_list_state.select(Some(last));
        self.persist_config();
        self.status_msg = "已添加域名".into();
    }

    pub fn delete_host_at_cursor(&mut self) {
        if self.hosts.len() <= 1 {
            self.status_msg = "至少保留一个域名".into();
            return;
        }
        let Some(i) = self.hosts_list_state.selected() else {
            return;
        };
        if i >= self.hosts.len() {
            return;
        }
        let was_active = i == self.base_url_index;
        self.hosts.remove(i);
        if self.base_url_index > i {
            self.base_url_index -= 1;
        } else if was_active {
            self.base_url_index = self.base_url_index.min(self.hosts.len().saturating_sub(1));
        }
        let max = self.hosts.len().saturating_sub(1);
        let sel = self.hosts_list_state.selected().unwrap_or(0).min(max);
        self.hosts_list_state.select(Some(sel));
        self.persist_config();
        self.status_msg = "已删除域名".into();
    }

    pub fn set_active_host_from_cursor(&mut self) {
        let Some(i) = self.hosts_list_state.selected() else {
            return;
        };
        if i < self.hosts.len() {
            self.base_url_index = i;
            self.persist_config();
            self.status_msg = format!("当前请求: {}", self.active_base_url());
        }
    }

    pub fn begin_edit_host_url(&mut self) {
        let Some(i) = self.hosts_list_state.selected() else {
            return;
        };
        if i < self.hosts.len() {
            self.host_buf = self.hosts[i].url.clone();
            self.hosts_edit_kind = Some(HostsEditKind::Url);
            self.hosts_edit_line = true;
            self.host_cursor_char = self.host_buf.chars().count();
        }
    }

    pub fn begin_edit_host_description(&mut self) {
        let Some(i) = self.hosts_list_state.selected() else {
            return;
        };
        if i < self.hosts.len() {
            self.host_buf = self.hosts[i].description.clone().unwrap_or_default();
            self.hosts_edit_kind = Some(HostsEditKind::Description);
            self.hosts_edit_line = true;
            self.host_cursor_char = self.host_buf.chars().count();
        }
    }

    pub fn cancel_host_line_edit(&mut self) {
        self.hosts_edit_line = false;
        self.hosts_edit_kind = None;
    }

    pub fn host_cursor_left(&mut self) {
        if self.host_cursor_char > 0 {
            self.host_cursor_char -= 1;
        }
    }

    pub fn host_cursor_right(&mut self) {
        let n = self.host_buf.chars().count();
        if self.host_cursor_char < n {
            self.host_cursor_char += 1;
        }
    }

    pub fn host_insert_char(&mut self, c: char) {
        let b = super::body_cursor::char_byte_index(&self.host_buf, self.host_cursor_char);
        self.host_buf.insert(b, c);
        self.host_cursor_char += 1;
    }

    pub fn host_insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.host_buf, self.host_cursor_char);
        self.host_buf.insert_str(b, s);
        self.host_cursor_char += s.chars().count();
    }

    pub fn host_backspace(&mut self) {
        if self.host_cursor_char == 0 {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.host_buf, self.host_cursor_char - 1);
        let ch = self.host_buf[b..].chars().next().unwrap();
        self.host_buf.drain(b..b + ch.len_utf8());
        self.host_cursor_char -= 1;
    }

    pub fn host_delete_forward(&mut self) {
        let n = self.host_buf.chars().count();
        if self.host_cursor_char >= n {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.host_buf, self.host_cursor_char);
        let ch = self.host_buf[b..].chars().next().unwrap();
        self.host_buf.drain(b..b + ch.len_utf8());
    }

    pub fn commit_host_line_edit(&mut self) {
        let Some(i) = self.hosts_list_state.selected() else {
            return;
        };
        let Some(kind) = self.hosts_edit_kind else {
            return;
        };
        if i >= self.hosts.len() {
            return;
        }
        match kind {
            HostsEditKind::Url => {
                let mut s = self.host_buf.trim().to_string();
                if s.is_empty() {
                    s = "http://localhost:8080".into();
                }
                self.hosts[i].url = s;
            }
            HostsEditKind::Description => {
                let t = self.host_buf.trim();
                self.hosts[i].description = if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                };
            }
        }
        self.persist_config();
        self.hosts_edit_line = false;
        self.hosts_edit_kind = None;
        self.status_msg = "已保存".into();
    }

    /// 实际会随请求发送的头行数（`name` 非空）。
    pub fn effective_request_header_count(&self) -> usize {
        self.request_headers
            .iter()
            .filter(|h| !h.name.trim().is_empty())
            .count()
    }

    /// 发给 `send_http` 的附加头。
    #[allow(dead_code)]
    pub fn extra_headers_for_http(&self) -> Vec<(String, String)> {
        self.request_headers
            .iter()
            .filter(|h| !h.name.trim().is_empty())
            .map(|h| (h.name.trim().to_string(), h.value.clone()))
            .collect()
    }

    /// 合并顺序：全局头（`a`）→ 详情按方法预设头 → 扫描到的 `@RequestHeader`；后者同名覆盖前者。
    /// 空串或全空白值不会加入请求。
    #[allow(dead_code)]
    pub fn merged_http_headers(&self) -> Vec<(String, String)> {
        use std::collections::HashMap;
        let mut acc: HashMap<String, (String, String)> = HashMap::new();
        for (k, v) in self.extra_headers_for_http() {
            if v.trim().is_empty() {
                continue;
            }
            acc.insert(k.to_ascii_lowercase(), (k, v));
        }
        for (k, v) in &self.method_detail_headers {
            if v.trim().is_empty() {
                continue;
            }
            acc.insert(k.to_ascii_lowercase(), (k.clone(), v.clone()));
        }
        for (k, v) in &self.extra_request_headers {
            if k.trim().is_empty() || v.trim().is_empty() {
                continue;
            }
            acc.insert(
                k.trim().to_ascii_lowercase(),
                (k.trim().to_string(), v.clone()),
            );
        }
        if let Some(api) = &self.current_api {
            for p in &api.headers {
                let v = self
                    .header_vals
                    .get(&p.name)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if v.trim().is_empty() {
                    continue;
                }
                acc.insert(p.name.to_ascii_lowercase(), (p.name.clone(), v.to_string()));
            }
            if let Some(cookie_header) = self.compose_cookie_header_value() {
                let key = "cookie".to_string();
                let merged = match acc.remove(&key) {
                    Some((orig_name, existing)) if !existing.trim().is_empty() => {
                        (orig_name, format!("{existing}; {cookie_header}"))
                    }
                    Some((orig_name, _)) => (orig_name, cookie_header),
                    None => ("Cookie".to_string(), cookie_header),
                };
                acc.insert(key, merged);
            }
        }
        acc.into_values().collect()
    }

    #[allow(dead_code)]
    fn compose_cookie_header_value(&self) -> Option<String> {
        let api = self.current_api.as_ref()?;
        let mut parts = Vec::new();
        for p in &api.cookie_params {
            let v = self
                .cookie_vals
                .get(&p.name)
                .map(|s| s.trim())
                .unwrap_or("");
            if v.is_empty() {
                continue;
            }
            parts.push(format!("{}={}", p.name, v));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    /// 发送前的最小必填校验：拦住明显缺失的 Path / Query / Header / Body。
    #[allow(dead_code)]
    pub fn validate_request_before_send(&self) -> Result<(), String> {
        let Some(api) = &self.current_api else {
            return Err("请先选择接口".into());
        };

        let mut missing = Vec::new();

        for p in &api.path_params {
            let empty = self
                .path_vals
                .get(&p.name)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true);
            if p.required && empty {
                missing.push(format!("Path `{}`", p.name));
            }
        }

        for p in &api.query_params {
            let empty = self
                .query_vals
                .get(&p.name)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true);
            if p.required && empty {
                missing.push(format!("Query `{}`", p.name));
            }
        }

        for p in &api.headers {
            let empty = self
                .header_vals
                .get(&p.name)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true);
            if p.required && empty {
                missing.push(format!("Header `{}`", p.name));
            }
        }

        for p in &api.cookie_params {
            let empty = self
                .cookie_vals
                .get(&p.name)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true);
            if p.required && empty {
                missing.push(format!("Cookie `{}`", p.name));
            }
        }

        if let Some(body) = &api.body_binding {
            if body.required && self.body_draft.trim().is_empty() {
                missing.push(format!("Body `{}`", body.name));
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("缺少必填参数: {}", missing.join("、")))
        }
    }

    pub fn prepare_request_for_send(
        &mut self,
    ) -> Result<(String, String, Vec<(String, String)>, Option<String>), String> {
        self.ensure_current_api_selected()
            .map_err(|m| m.to_string())?;
        self.validate_request_before_send()?;
        if self
            .current_api
            .as_ref()
            .and_then(|api| api.body_binding.as_ref())
            .is_some()
        {
            self.validate_body_editor_json()?;
        }

        let api = self
            .current_api
            .as_ref()
            .ok_or_else(|| "请先选择接口".to_string())?;
        let full_url = crate::http_exec::compose_url(
            self.active_base_url(),
            &api.path,
            &self.path_vals,
            &self.merged_query_for_url(),
        )?;
        let headers = self.merged_http_headers();
        let body = if api.body_binding.is_some() {
            Some(self.body_draft.clone())
        } else {
            None
        };
        Ok((api.http_method.clone(), full_url, headers, body))
    }

    pub fn set_last_http_response(&mut self, h: HttpResult) {
        self.response_json = serde_json::from_str(&h.body).ok();
        self.response_json_expanded.clear();
        self.response_focus = 0;
        self.last_http = Some(h);
        self.response_scroll = 0;
        self.response_view_scroll = 0;
        self.response_view_max_scroll = 0;
    }

    pub fn open_response_view(&mut self) -> Result<(), &'static str> {
        if self.last_http.is_none() {
            return Err("当前还没有响应可查看");
        }
        self.response_view_open = true;
        self.response_view_scroll = 0;
        self.response_view_max_scroll = 0;
        Ok(())
    }

    pub fn set_response_view_max_scroll(&mut self, max_scroll: u16) {
        self.response_view_max_scroll = max_scroll;
        self.response_view_scroll = self.response_view_scroll.min(max_scroll);
    }

    pub fn scroll_response_view_by(&mut self, delta: i32) {
        let next = (self.response_view_scroll as i32 + delta)
            .clamp(0, self.response_view_max_scroll as i32) as u16;
        self.response_view_scroll = next;
    }

    pub fn response_popup_text(&self) -> String {
        let Some(h) = &self.last_http else {
            return "尚无响应。".into();
        };
        let body = serde_json::from_str::<serde_json::Value>(&h.body)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| h.body.clone());
        format!(
            "HTTP {}  |  {} ms\n{}\n\n{}",
            h.status, h.elapsed_ms, h.headers_text, body
        )
    }

    pub fn response_json_lines(&self) -> Vec<ResponseJsonLine> {
        let Some(value) = &self.response_json else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.push_response_json_lines(value, "", None, 0, &mut out);
        out
    }

    pub fn move_response_focus(&mut self, delta: isize) {
        let lines = self.response_json_lines();
        if lines.is_empty() {
            return;
        }
        let max = lines.len().saturating_sub(1) as isize;
        self.response_focus = (self.response_focus as isize + delta).clamp(0, max) as usize;
    }

    pub fn ensure_response_focus_visible(&mut self, visible_height: u16) {
        let Some(line_idx) = self.response_focus_line_index() else {
            return;
        };
        let vh = visible_height.max(1) as usize;
        let total = self.response_render_line_count().max(1);
        let max_scroll = total.saturating_sub(vh);
        let top = self.response_scroll as usize;
        if line_idx < top {
            self.response_scroll = line_idx.min(max_scroll) as u16;
        } else if line_idx >= top + vh {
            self.response_scroll = (line_idx + 1).saturating_sub(vh).min(max_scroll) as u16;
        } else if top > max_scroll {
            self.response_scroll = max_scroll as u16;
        }
    }

    pub fn toggle_response_node_at_focus(&mut self) -> bool {
        let lines = self.response_json_lines();
        let Some(line) = lines.get(self.response_focus) else {
            return false;
        };
        if !line.expandable {
            return false;
        }
        if !self.response_json_expanded.insert(line.path.clone()) {
            self.response_json_expanded.remove(&line.path);
        }
        true
    }

    fn response_focus_line_index(&self) -> Option<usize> {
        self.response_json.as_ref()?;
        let header_lines = self
            .last_http
            .as_ref()
            .map(|h| h.headers_text.lines().count())
            .unwrap_or(0);
        Some(header_lines + 2 + self.response_focus)
    }

    fn response_render_line_count(&self) -> usize {
        let Some(h) = self.last_http.as_ref() else {
            return 1;
        };
        if self.response_json.is_some() {
            2 + h.headers_text.lines().count() + self.response_json_lines().len()
        } else {
            let body = h.body.lines().count();
            2 + h.headers_text.lines().count() + body
        }
    }

    fn push_response_json_lines(
        &self,
        value: &serde_json::Value,
        path: &str,
        key: Option<&str>,
        depth: usize,
        out: &mut Vec<ResponseJsonLine>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(k) = key {
                    let expanded = self.response_json_expanded.contains(path);
                    out.push(ResponseJsonLine {
                        depth,
                        path: path.to_string(),
                        label: format!("{}: {}", k, object_summary(map.len())),
                        expandable: !map.is_empty(),
                        expanded,
                    });
                    if !expanded {
                        return;
                    }
                }
                for (child_key, child_value) in map {
                    let child_path = format!("{}/{}", path, escape_json_pointer(child_key));
                    self.push_response_json_lines(
                        child_value,
                        &child_path,
                        Some(child_key),
                        depth + usize::from(key.is_some()),
                        out,
                    );
                }
            }
            serde_json::Value::Array(items) => {
                if let Some(k) = key {
                    let expanded = self.response_json_expanded.contains(path);
                    out.push(ResponseJsonLine {
                        depth,
                        path: path.to_string(),
                        label: format!("{}: {}", k, array_summary(items.len())),
                        expandable: !items.is_empty(),
                        expanded,
                    });
                    if !expanded {
                        return;
                    }
                }
                for (idx, child_value) in items.iter().enumerate() {
                    let idx_label = format!("[{}]", idx);
                    let child_path = format!("{}/{}", path, idx);
                    self.push_response_json_lines(
                        child_value,
                        &child_path,
                        Some(&idx_label),
                        depth + usize::from(key.is_some()),
                        out,
                    );
                }
            }
            _ => {
                let label = match key {
                    Some(k) => format!("{}: {}", k, scalar_summary(value)),
                    None => scalar_summary(value),
                };
                out.push(ResponseJsonLine {
                    depth,
                    path: path.to_string(),
                    label,
                    expandable: false,
                    expanded: false,
                });
            }
        }
    }

    pub fn open_headers_panel(&mut self) {
        self.headers_panel_open = true;
        self.headers_edit_line = false;
        self.headers_edit_kind = None;
        if self.request_headers.is_empty() {
            self.headers_list_state.select(None);
        } else {
            let i = self
                .selected_request_header
                .min(self.request_headers.len() - 1);
            self.headers_list_state.select(Some(i));
        }
    }

    pub fn add_header_row(&mut self) {
        self.request_headers.push(HeaderEntry {
            name: "Authorization".into(),
            value: String::new(),
            description: None,
        });
        let last = self.request_headers.len() - 1;
        self.headers_list_state.select(Some(last));
        self.selected_request_header = last;
        self.persist_config();
        self.status_msg = "已添加请求头".into();
    }

    pub fn delete_header_at_cursor(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        if i >= self.request_headers.len() {
            return;
        }
        self.request_headers.remove(i);
        if self.selected_request_header > i {
            self.selected_request_header -= 1;
        } else if self.selected_request_header >= self.request_headers.len() {
            self.selected_request_header = self.request_headers.len().saturating_sub(1);
        }
        let sel = if self.request_headers.is_empty() {
            None
        } else {
            let new_i = i.min(self.request_headers.len() - 1);
            Some(new_i)
        };
        self.headers_list_state.select(sel);
        self.persist_config();
        self.status_msg = "已删除请求头".into();
    }

    pub fn set_active_header_from_cursor(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        if i < self.request_headers.len() {
            self.selected_request_header = i;
            self.persist_config();
            let label = self.request_headers[i].display_label();
            self.status_msg = format!("当前请求头: {label}");
        }
    }

    pub fn begin_edit_header_name(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        if i < self.request_headers.len() {
            self.header_buf = self.request_headers[i].name.clone();
            self.headers_edit_kind = Some(RequestHeaderEditKind::Name);
            self.headers_edit_line = true;
            self.header_cursor_char = self.header_buf.chars().count();
        }
    }

    pub fn begin_edit_header_value(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        if i < self.request_headers.len() {
            self.header_buf = self.request_headers[i].value.clone();
            self.headers_edit_kind = Some(RequestHeaderEditKind::Value);
            self.headers_edit_line = true;
            self.header_cursor_char = self.header_buf.chars().count();
        }
    }

    pub fn begin_edit_header_description(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        if i < self.request_headers.len() {
            self.header_buf = self.request_headers[i]
                .description
                .clone()
                .unwrap_or_default();
            self.headers_edit_kind = Some(RequestHeaderEditKind::Description);
            self.headers_edit_line = true;
            self.header_cursor_char = self.header_buf.chars().count();
        }
    }

    pub fn cancel_header_line_edit(&mut self) {
        self.headers_edit_line = false;
        self.headers_edit_kind = None;
    }

    pub fn header_cursor_left(&mut self) {
        if self.header_cursor_char > 0 {
            self.header_cursor_char -= 1;
        }
    }

    pub fn header_cursor_right(&mut self) {
        let n = self.header_buf.chars().count();
        if self.header_cursor_char < n {
            self.header_cursor_char += 1;
        }
    }

    pub fn header_insert_char(&mut self, c: char) {
        let b = super::body_cursor::char_byte_index(&self.header_buf, self.header_cursor_char);
        self.header_buf.insert(b, c);
        self.header_cursor_char += 1;
    }

    pub fn header_insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.header_buf, self.header_cursor_char);
        self.header_buf.insert_str(b, s);
        self.header_cursor_char += s.chars().count();
    }

    pub fn header_backspace(&mut self) {
        if self.header_cursor_char == 0 {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.header_buf, self.header_cursor_char - 1);
        let ch = self.header_buf[b..].chars().next().unwrap();
        self.header_buf.drain(b..b + ch.len_utf8());
        self.header_cursor_char -= 1;
    }

    pub fn header_delete_forward(&mut self) {
        let n = self.header_buf.chars().count();
        if self.header_cursor_char >= n {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.header_buf, self.header_cursor_char);
        let ch = self.header_buf[b..].chars().next().unwrap();
        self.header_buf.drain(b..b + ch.len_utf8());
    }

    pub fn commit_header_line_edit(&mut self) {
        let Some(i) = self.headers_list_state.selected() else {
            return;
        };
        let Some(kind) = self.headers_edit_kind else {
            return;
        };
        if i >= self.request_headers.len() {
            return;
        }
        match kind {
            RequestHeaderEditKind::Name => {
                self.request_headers[i].name = self.header_buf.trim().to_string();
            }
            RequestHeaderEditKind::Value => {
                self.request_headers[i].value = self.header_buf.clone();
            }
            RequestHeaderEditKind::Description => {
                let t = self.header_buf.trim();
                self.request_headers[i].description = if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                };
            }
        }
        self.persist_config();
        self.headers_edit_line = false;
        self.headers_edit_kind = None;
        self.status_msg = "已保存".into();
    }

    pub fn apply_scan(&mut self, report: ScanReport) {
        self.apis = report.apis;
        self.scan_errors = report.file_errors;
        self.refresh_filter();
        let warn_n = self.scan_errors.len();
        if warn_n > 0 {
            self.status_msg = format!("已索引 {} 个接口 · 扫描警告 {} 条", self.apis.len(), warn_n);
        } else {
            self.status_msg = format!("已索引 {} 个接口", self.apis.len());
        }
    }

    pub fn apply_source_cache(&mut self, cache: JavaSourceCache) {
        for e in &cache.load_errors {
            self.scan_errors.push(e.clone());
        }
        self.source_cache = Some(Arc::new(cache));
        self.source_cache_loading = false;
        self.sync_detail_from_selection();
    }

    pub fn refresh_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .apis
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                q.is_empty()
                    || a.path.to_lowercase().contains(&q)
                    || a.http_method.to_lowercase().contains(&q)
                    || a.module.to_lowercase().contains(&q)
                    || a.name.to_lowercase().contains(&q)
                    || a.description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || a.class_doc
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || a.openapi_tag
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || a.project_bucket.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();

        self.rebuild_list_rows();
        if self.list_rows.is_empty() {
            self.list_state.select(None);
            self.current_api = None;
        } else {
            let sel = self
                .first_endpoint_row_index()
                .unwrap_or(0)
                .min(self.list_rows.len() - 1);
            self.list_state.select(Some(sel));
            self.sync_detail_from_selection();
        }
    }

    fn first_endpoint_row_index(&self) -> Option<usize> {
        self.list_rows
            .iter()
            .position(|r| matches!(r, ListRow::Endpoint { .. }))
    }

    pub fn rebuild_list_rows(&mut self) {
        self.list_rows.clear();
        if self.filtered.is_empty() {
            return;
        }
        match self.list_group_mode {
            ListGroupMode::Flat => {
                for &i in &self.filtered {
                    self.list_rows.push(ListRow::Endpoint { api_index: i });
                }
            }
            ListGroupMode::ByController => {
                let mut idxs: Vec<usize> = self.filtered.clone();
                idxs.sort_by(|&a, &b| {
                    self.apis[a]
                        .module
                        .cmp(&self.apis[b].module)
                        .then_with(|| self.apis[a].line.cmp(&self.apis[b].line))
                        .then_with(|| self.apis[a].path.cmp(&self.apis[b].path))
                });
                if self.pending_collapse_all_modules {
                    self.pending_collapse_all_modules = false;
                    self.collapsed_modules =
                        idxs.iter().map(|&i| self.apis[i].module.clone()).collect();
                }
                let mut last: Option<&str> = None;
                for &i in &idxs {
                    let m = self.apis[i].module.as_str();
                    if last != Some(m) {
                        let label = self.apis[i].module_group_label();
                        self.list_rows.push(ListRow::ClassHeader {
                            scope_bucket: None,
                            module: m.to_string(),
                            label,
                        });
                        last = Some(m);
                    }
                    if !self.collapsed_modules.contains(m) {
                        self.list_rows.push(ListRow::Endpoint { api_index: i });
                    }
                }
            }
            ListGroupMode::ByProjectFolder => {
                let mut idxs: Vec<usize> = self.filtered.clone();
                idxs.sort_by(|&a, &b| {
                    self.apis[a]
                        .project_bucket
                        .cmp(&self.apis[b].project_bucket)
                        .then_with(|| self.apis[a].module.cmp(&self.apis[b].module))
                        .then_with(|| self.apis[a].line.cmp(&self.apis[b].line))
                        .then_with(|| self.apis[a].path.cmp(&self.apis[b].path))
                });
                if self.pending_collapse_all_projects {
                    self.pending_collapse_all_projects = false;
                    self.collapsed_project_buckets = idxs
                        .iter()
                        .map(|&i| self.apis[i].project_bucket.clone())
                        .collect();
                }
                if self.pending_collapse_all_classes_in_projects {
                    self.pending_collapse_all_classes_in_projects = false;
                    self.collapsed_class_in_project = idxs
                        .iter()
                        .map(|&i| {
                            let a = &self.apis[i];
                            (a.project_bucket.clone(), a.module.clone())
                        })
                        .collect();
                }
                let mut last_bucket: Option<&str> = None;
                let mut last_module_in_bucket: Option<&str> = None;
                for &i in &idxs {
                    let api = &self.apis[i];
                    let b = api.project_bucket.as_str();
                    let m = api.module.as_str();
                    if last_bucket != Some(b) {
                        self.list_rows.push(ListRow::ProjectHeader(b.to_string()));
                        last_bucket = Some(b);
                        last_module_in_bucket = None;
                    }
                    if self.collapsed_project_buckets.contains(b) {
                        continue;
                    }
                    if last_module_in_bucket != Some(m) {
                        let label = api.module_group_label();
                        self.list_rows.push(ListRow::ClassHeader {
                            scope_bucket: Some(b.to_string()),
                            module: m.to_string(),
                            label,
                        });
                        last_module_in_bucket = Some(m);
                    }
                    let class_key = (b.to_string(), m.to_string());
                    if !self.collapsed_class_in_project.contains(&class_key) {
                        self.list_rows.push(ListRow::Endpoint { api_index: i });
                    }
                }
            }
        }
    }

    /// `g`：按项目目录 → 平铺 → 按类 → 按项目目录。
    #[allow(dead_code)]
    pub fn cycle_list_group_mode(&mut self) {
        use ListGroupMode::*;
        self.list_group_mode = match self.list_group_mode {
            ByController => {
                self.collapsed_modules.clear();
                self.collapsed_class_in_project.clear();
                self.pending_collapse_all_projects = true;
                self.pending_collapse_all_classes_in_projects = true;
                ByProjectFolder
            }
            ByProjectFolder => {
                self.collapsed_project_buckets.clear();
                self.collapsed_class_in_project.clear();
                Flat
            }
            Flat => {
                self.pending_collapse_all_modules = true;
                self.collapsed_class_in_project.clear();
                ByController
            }
        };
        self.rebuild_list_rows();
        if self.list_rows.is_empty() {
            self.list_state.select(None);
            self.current_api = None;
        } else {
            let sel = self
                .first_endpoint_row_index()
                .unwrap_or(0)
                .min(self.list_rows.len() - 1);
            self.list_state.select(Some(sel));
            self.sync_detail_from_selection();
        }
    }

    /// 分组模式下选中类标题或项目目录标题时，折叠/展开下属接口行。
    pub fn toggle_collapse_selected_header(&mut self) -> bool {
        let Some(ix) = self.list_state.selected() else {
            return false;
        };
        match self.list_group_mode {
            ListGroupMode::Flat => return false,
            ListGroupMode::ByController => {
                let Some(ListRow::ClassHeader {
                    scope_bucket: None,
                    module,
                    ..
                }) = self.list_rows.get(ix)
                else {
                    return false;
                };
                let k = module.clone();
                if self.collapsed_modules.contains(&k) {
                    self.collapsed_modules.remove(&k);
                } else {
                    self.collapsed_modules.insert(k.clone());
                }
                self.rebuild_list_rows();
                if self.list_rows.is_empty() {
                    self.list_state.select(None);
                    self.current_api = None;
                    return true;
                }
                let sel = self
                    .list_rows
                    .iter()
                    .position(|r| {
                        matches!(r, ListRow::ClassHeader { scope_bucket: None, module: m, .. } if m == &k)
                    })
                    .unwrap_or(0)
                    .min(self.list_rows.len() - 1);
                self.list_state.select(Some(sel));
                self.sync_detail_from_selection();
            }
            ListGroupMode::ByProjectFolder => match self.list_rows.get(ix) {
                Some(ListRow::ProjectHeader(bucket)) => {
                    let k = bucket.clone();
                    if self.collapsed_project_buckets.contains(&k) {
                        self.collapsed_project_buckets.remove(&k);
                    } else {
                        self.collapsed_project_buckets.insert(k.clone());
                    }
                    self.rebuild_list_rows();
                    if self.list_rows.is_empty() {
                        self.list_state.select(None);
                        self.current_api = None;
                        return true;
                    }
                    let sel = self
                        .list_rows
                        .iter()
                        .position(|r| matches!(r, ListRow::ProjectHeader(b) if b == &k))
                        .unwrap_or(0)
                        .min(self.list_rows.len() - 1);
                    self.list_state.select(Some(sel));
                    self.sync_detail_from_selection();
                }
                Some(ListRow::ClassHeader {
                    scope_bucket: Some(bucket),
                    module,
                    ..
                }) => {
                    let bucket_owned = bucket.clone();
                    let module_owned = module.clone();
                    let key = (bucket_owned.clone(), module_owned.clone());
                    if self.collapsed_class_in_project.contains(&key) {
                        self.collapsed_class_in_project.remove(&key);
                    } else {
                        self.collapsed_class_in_project.insert(key.clone());
                    }
                    self.rebuild_list_rows();
                    if self.list_rows.is_empty() {
                        self.list_state.select(None);
                        self.current_api = None;
                        return true;
                    }
                    let sel = self
                            .list_rows
                            .iter()
                            .position(|r| {
                                matches!(
                                    r,
                                    ListRow::ClassHeader {
                                        scope_bucket: Some(b),
                                        module: m,
                                        ..
                                    } if b.as_str() == bucket_owned.as_str() && m.as_str() == module_owned.as_str()
                                )
                            })
                            .unwrap_or(0)
                            .min(self.list_rows.len() - 1);
                    self.list_state.select(Some(sel));
                    self.sync_detail_from_selection();
                }
                _ => return false,
            },
        }
        true
    }

    pub fn sync_detail_from_selection(&mut self) {
        let Some(row_ix) = self.list_state.selected() else {
            if self.current_api.is_some() {
                if self.request_editor_open {
                    self.save_request_editor_row();
                    self.request_editor_open = false;
                    self.request_editor_error = None;
                }
                self.save_current_api_draft();
                self.persist_config();
            }
            self.current_api = None;
            return;
        };
        let api_ix = match self.list_rows.get(row_ix) {
            Some(ListRow::Endpoint { api_index }) => *api_index,
            Some(ListRow::ClassHeader { .. }) | Some(ListRow::ProjectHeader(_)) => {
                if self.current_api.is_some() {
                    if self.request_editor_open {
                        self.save_request_editor_row();
                        self.request_editor_open = false;
                        self.request_editor_error = None;
                    }
                    self.save_current_api_draft();
                    self.persist_config();
                }
                self.current_api = None;
                return;
            }
            None => {
                if self.current_api.is_some() {
                    if self.request_editor_open {
                        self.save_request_editor_row();
                        self.request_editor_open = false;
                        self.request_editor_error = None;
                    }
                    self.save_current_api_draft();
                    self.persist_config();
                }
                self.current_api = None;
                return;
            }
        };
        let mut lazy_filled = false;
        if let Some(cache) = self.source_cache.clone() {
            lazy_filled = crate::scanner::ensure_request_body_resolved(
                &mut self.apis[api_ix],
                &cache,
                &self.project,
            );
        }
        let api = self.apis[api_ix].clone();
        let id_changed = self
            .current_api
            .as_ref()
            .map(|a| a.id != api.id)
            .unwrap_or(true);
        if id_changed {
            if self.request_editor_open {
                self.save_request_editor_row();
                self.request_editor_open = false;
                self.request_editor_error = None;
            }
            self.save_current_api_draft();
            self.persist_config();
            self.param_focus = None;
            self.detail_params_scroll = 0;
            self.detail_headers_scroll = 0;
            self.detail_body_scroll = 0;
            self.restore_request_state_for_api(&api);
            self.current_api = Some(api);
        } else if lazy_filled {
            self.detail_params_scroll = 0;
            self.detail_headers_scroll = 0;
            self.detail_body_scroll = 0;
            self.restore_request_state_for_api(&api);
            self.current_api = Some(api);
        }
    }

    fn save_current_api_draft(&mut self) {
        let Some(api) = &self.current_api else {
            return;
        };
        let draft_key = api.request_draft_key();
        self.request_drafts.insert(
            draft_key.clone(),
            ApiRequestDraft {
                path_vals: self.path_vals.clone(),
                query_vals: self.query_vals.clone(),
                extra_query_params: self.extra_query_params.clone(),
                method_detail_headers: self.method_detail_headers.clone(),
                extra_request_headers: self.extra_request_headers.clone(),
                header_vals: self.header_vals.clone(),
                cookie_vals: self.cookie_vals.clone(),
                body_draft: self.body_draft.clone(),
            },
        );
        if draft_key != api.id {
            self.request_drafts.remove(&api.id);
        }
    }

    fn restore_request_state_for_api(&mut self, api: &LocalApi) {
        self.restore_defaults_for_api(api);

        let draft = self.request_drafts.get(&api.request_draft_key()).cloned();
        let Some(draft) = draft else {
            return;
        };

        for p in &api.path_params {
            if let Some(v) = draft.path_vals.get(&p.name) {
                self.path_vals.insert(p.name.clone(), v.clone());
            }
        }
        for p in &api.query_params {
            if let Some(v) = draft.query_vals.get(&p.name) {
                self.query_vals.insert(p.name.clone(), v.clone());
            }
        }
        self.extra_query_params = draft.extra_query_params;

        let mut method_headers =
            default_method_detail_headers(&api.http_method, api.body_binding.is_some());
        for (name, value) in &mut method_headers {
            if let Some((_, saved)) = draft
                .method_detail_headers
                .iter()
                .find(|(saved_name, _)| saved_name.eq_ignore_ascii_case(name))
            {
                *value = saved.clone();
            }
        }
        self.method_detail_headers = method_headers;
        self.extra_request_headers = draft.extra_request_headers;

        for p in &api.headers {
            if let Some(v) = draft.header_vals.get(&p.name) {
                self.header_vals.insert(p.name.clone(), v.clone());
            }
        }
        for p in &api.cookie_params {
            if let Some(v) = draft.cookie_vals.get(&p.name) {
                self.cookie_vals.insert(p.name.clone(), v.clone());
            }
        }
        if api.body_binding.is_some() {
            self.body_draft = draft.body_draft;
        }
    }

    pub fn persist_current_request_draft(&mut self) {
        self.save_current_api_draft();
        self.persist_config();
    }

    fn restore_defaults_for_api(&mut self, api: &LocalApi) {
        self.path_vals.clear();
        for p in &api.path_params {
            self.path_vals.insert(
                p.name.clone(),
                p.default_value.clone().unwrap_or_else(|| "1".into()),
            );
        }
        self.query_vals.clear();
        for p in &api.query_params {
            self.query_vals
                .insert(p.name.clone(), p.default_value.clone().unwrap_or_default());
        }
        self.extra_query_params.clear();
        self.method_detail_headers =
            default_method_detail_headers(&api.http_method, api.body_binding.is_some());
        self.extra_request_headers.clear();
        self.header_vals.clear();
        for p in &api.headers {
            self.header_vals
                .insert(p.name.clone(), p.default_value.clone().unwrap_or_default());
        }
        self.cookie_vals.clear();
        for p in &api.cookie_params {
            self.cookie_vals
                .insert(p.name.clone(), p.default_value.clone().unwrap_or_default());
        }
        self.body_draft = api
            .body
            .as_ref()
            .map(|v| serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into()))
            .unwrap_or_else(|| "{}".into());
    }

    pub fn move_sel(&mut self, delta: isize) {
        if self.list_rows.is_empty() {
            return;
        }
        let n = self.list_rows.len();
        let cur = self.list_state.selected().unwrap_or(0);
        let ni = (cur as isize + delta).clamp(0, (n - 1) as isize) as usize;
        self.list_state.select(Some(ni));
        self.sync_detail_from_selection();
    }

    /// 发送与预览 URL 使用的 Query：扫描默认值 + 手工附加（同名时以后者为准）。
    pub fn merged_query_for_url(&self) -> HashMap<String, String> {
        let mut m = self.query_vals.clone();
        for (k, v) in &self.extra_query_params {
            let kt = k.trim();
            if kt.is_empty() {
                continue;
            }
            m.insert(kt.to_string(), v.clone());
        }
        m
    }

    pub fn request_editor_on_extra_query_row(&self) -> bool {
        let Some(api) = &self.current_api else {
            return false;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        en > 0 && f >= pn + qn && f < pn + qn + en
    }

    pub fn request_editor_on_extra_header_row(&self) -> bool {
        let Some(api) = &self.current_api else {
            return false;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        xhn > 0 && f >= pn + qn + en + hn && f < pn + qn + en + hn + xhn
    }

    /// 当前请求编辑弹窗所允许编辑的行范围（按 Params / Headers / Body 子模块分开）。
    pub fn request_editor_bounds(&self) -> Option<(usize, usize)> {
        let api = self.current_api.as_ref()?;
        let params_end =
            api.path_params.len() + api.query_params.len() + self.extra_query_params.len();
        let headers_end = params_end
            + self.method_detail_headers.len()
            + self.extra_request_headers.len()
            + api.cookie_params.len()
            + api.headers.len();
        let total = self.request_sheet_row_count();
        match self.detail_pane {
            DetailPane::Params if params_end > 0 => Some((0, params_end)),
            DetailPane::Headers if headers_end > params_end => Some((params_end, headers_end)),
            DetailPane::Body if api.body_binding.is_some() && total > 0 => Some((total - 1, total)),
            _ => None,
        }
    }

    /// 「+」：在扫描 Query 之后追加一行手工 Query（先编辑参数名，Tab 切到值）。
    pub fn add_extra_query_param(&mut self) {
        self.save_request_editor_row();
        self.extra_query_params.push((String::new(), String::new()));
        let Some(api) = &self.current_api else {
            return;
        };
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        self.request_editor_focus = pn + qn + en - 1;
        self.request_editor_extra_kv = true;
        self.load_request_editor_row();
    }

    pub fn add_extra_request_header(&mut self) {
        self.save_request_editor_row();
        self.extra_request_headers
            .push(("X-New-Header".into(), String::new()));
        let Some(api) = &self.current_api else {
            return;
        };
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        self.request_editor_focus = pn + qn + en + hn + xhn - 1;
        self.request_editor_extra_kv = true;
        self.load_request_editor_row();
    }

    /// 焦点在附加 Query 行时按 `d`：删除该行。成功返回 `true`。
    pub fn delete_extra_query_row_at_focus(&mut self) -> bool {
        if !self.request_editor_on_extra_query_row() {
            return false;
        }
        self.save_request_editor_row();
        let Some(api) = self.current_api.as_ref() else {
            return false;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let idx = f - pn - qn;
        if idx >= self.extra_query_params.len() {
            return false;
        }
        self.extra_query_params.remove(idx);
        self.request_editor_extra_kv = false;
        if self.detail_pane == DetailPane::Params && pn + qn + self.extra_query_params.len() == 0 {
            self.extra_query_params.push((String::new(), String::new()));
            self.request_editor_focus = pn + qn;
            self.request_editor_extra_kv = true;
            self.load_request_editor_row();
            return true;
        }
        let Some((start, end)) = self.request_editor_bounds() else {
            return true;
        };
        self.request_editor_focus = self.request_editor_focus.clamp(start, end - 1);
        self.load_request_editor_row();
        true
    }

    pub fn delete_extra_header_row_at_focus(&mut self) -> bool {
        if !self.request_editor_on_extra_header_row() {
            return false;
        }
        self.save_request_editor_row();
        let Some(api) = self.current_api.as_ref() else {
            return false;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let idx = f - pn - qn - en - hn;
        if idx >= self.extra_request_headers.len() {
            return false;
        }
        self.extra_request_headers.remove(idx);
        self.request_editor_extra_kv = false;
        let Some((start, end)) = self.request_editor_bounds() else {
            return true;
        };
        self.request_editor_focus = self.request_editor_focus.clamp(start, end - 1);
        self.load_request_editor_row();
        true
    }

    pub fn request_editor_is_body_row(&self) -> bool {
        let Some(api) = &self.current_api else {
            return false;
        };
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        let cn = api.cookie_params.len();
        let rhn = api.headers.len();
        api.body_binding.is_some()
            && self.request_editor_focus >= pn + qn + en + hn + xhn + cn + rhn
    }

    pub fn save_request_editor_row(&mut self) {
        let Some(api) = &self.current_api else {
            return;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        let cn = api.cookie_params.len();
        let rhn = api.headers.len();
        if f < pn {
            let name = api.path_params[f].name.clone();
            self.path_vals.insert(name, self.param_edit_buf.clone());
        } else if f < pn + qn {
            let name = api.query_params[f - pn].name.clone();
            self.query_vals.insert(name, self.param_edit_buf.clone());
        } else if f < pn + qn + en {
            let row = f - pn - qn;
            if let Some(slot) = self.extra_query_params.get_mut(row) {
                if self.request_editor_extra_kv {
                    slot.0 = self.param_edit_buf.clone();
                } else {
                    slot.1 = self.param_edit_buf.clone();
                }
            }
        } else if f < pn + qn + en + hn {
            let row = f - pn - qn - en;
            if let Some(slot) = self.method_detail_headers.get_mut(row) {
                slot.1 = self.param_edit_buf.clone();
            }
        } else if f < pn + qn + en + hn + xhn {
            let row = f - pn - qn - en - hn;
            if let Some(slot) = self.extra_request_headers.get_mut(row) {
                if self.request_editor_extra_kv {
                    slot.0 = self.param_edit_buf.clone();
                } else {
                    slot.1 = self.param_edit_buf.clone();
                }
            }
        } else if f < pn + qn + en + hn + xhn + cn {
            let row = f - pn - qn - en - hn - xhn;
            let name = api.cookie_params[row].name.clone();
            self.cookie_vals.insert(name, self.param_edit_buf.clone());
        } else if f < pn + qn + en + hn + xhn + cn + rhn {
            let row = f - pn - qn - en - hn - xhn - cn;
            let name = api.headers[row].name.clone();
            self.header_vals.insert(name, self.param_edit_buf.clone());
        } else if api.body_binding.is_some() {
            self.body_draft = self.body_buf.clone();
        }
    }

    pub fn load_request_editor_row(&mut self) {
        let Some(api) = &self.current_api else {
            return;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        let cn = api.cookie_params.len();
        let rhn = api.headers.len();
        if f < pn {
            let name = &api.path_params[f].name;
            self.param_edit_buf = self.path_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn {
            let name = &api.query_params[f - pn].name;
            self.param_edit_buf = self.query_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + en {
            let row = f - pn - qn;
            let (ref k, ref v) = self.extra_query_params[row];
            self.param_edit_buf = if self.request_editor_extra_kv {
                k.clone()
            } else {
                v.clone()
            };
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + en + hn {
            let row = f - pn - qn - en;
            self.param_edit_buf = self
                .method_detail_headers
                .get(row)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + en + hn + xhn {
            let row = f - pn - qn - en - hn;
            let (ref k, ref v) = self.extra_request_headers[row];
            self.param_edit_buf = if self.request_editor_extra_kv {
                k.clone()
            } else {
                v.clone()
            };
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + en + hn + xhn + cn {
            let row = f - pn - qn - en - hn - xhn;
            let name = &api.cookie_params[row].name;
            self.param_edit_buf = self.cookie_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + en + hn + xhn + cn + rhn {
            let row = f - pn - qn - en - hn - xhn - cn;
            let name = &api.headers[row].name;
            self.param_edit_buf = self.header_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else {
            self.body_buf = self.body_draft.clone();
            self.body_cursor_char = self.body_buf.chars().count();
        }
    }

    pub fn body_cursor_up(&mut self) {
        self.body_cursor_char =
            super::body_cursor::cursor_up(&self.body_buf, self.body_cursor_char);
    }

    pub fn body_cursor_down(&mut self) {
        self.body_cursor_char =
            super::body_cursor::cursor_down(&self.body_buf, self.body_cursor_char);
    }

    pub fn body_cursor_left(&mut self) {
        if self.body_cursor_char > 0 {
            self.body_cursor_char -= 1;
        }
    }

    pub fn body_cursor_right(&mut self) {
        let n = self.body_buf.chars().count();
        if self.body_cursor_char < n {
            self.body_cursor_char += 1;
        }
    }

    pub fn body_insert_char(&mut self, c: char) {
        let b = super::body_cursor::char_byte_index(&self.body_buf, self.body_cursor_char);
        self.body_buf.insert(b, c);
        self.body_cursor_char += 1;
    }

    pub fn body_insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.body_buf, self.body_cursor_char);
        self.body_buf.insert_str(b, s);
        self.body_cursor_char += s.chars().count();
    }

    pub fn body_backspace(&mut self) {
        if self.body_cursor_char == 0 {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.body_buf, self.body_cursor_char - 1);
        let ch = self.body_buf[b..].chars().next().unwrap();
        self.body_buf.drain(b..b + ch.len_utf8());
        self.body_cursor_char -= 1;
    }

    pub fn body_delete_forward(&mut self) {
        let n = self.body_buf.chars().count();
        if self.body_cursor_char >= n {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.body_buf, self.body_cursor_char);
        let ch = self.body_buf[b..].chars().next().unwrap();
        self.body_buf.drain(b..b + ch.len_utf8());
    }

    pub fn clear_body_editor(&mut self) {
        self.body_buf.clear();
        self.body_cursor_char = 0;
    }

    pub fn validate_body_editor_json(&self) -> Result<(), String> {
        if self.body_buf.trim().is_empty() {
            return Ok(());
        }
        serde_json::from_str::<serde_json::Value>(&self.body_buf)
            .map(|_| ())
            .map_err(|e| format!("Body JSON 格式错误: {e}"))
    }

    pub fn param_cursor_left(&mut self) {
        if self.param_cursor_char > 0 {
            self.param_cursor_char -= 1;
        }
    }

    pub fn param_cursor_right(&mut self) {
        let n = self.param_edit_buf.chars().count();
        if self.param_cursor_char < n {
            self.param_cursor_char += 1;
        }
    }

    pub fn param_insert_char(&mut self, c: char) {
        let b = super::body_cursor::char_byte_index(&self.param_edit_buf, self.param_cursor_char);
        self.param_edit_buf.insert(b, c);
        self.param_cursor_char += 1;
    }

    pub fn param_insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.param_edit_buf, self.param_cursor_char);
        self.param_edit_buf.insert_str(b, s);
        self.param_cursor_char += s.chars().count();
    }

    pub fn param_backspace(&mut self) {
        if self.param_cursor_char == 0 {
            return;
        }
        let b =
            super::body_cursor::char_byte_index(&self.param_edit_buf, self.param_cursor_char - 1);
        let ch = self.param_edit_buf[b..].chars().next().unwrap();
        self.param_edit_buf.drain(b..b + ch.len_utf8());
        self.param_cursor_char -= 1;
    }

    pub fn param_delete_forward(&mut self) {
        let n = self.param_edit_buf.chars().count();
        if self.param_cursor_char >= n {
            return;
        }
        let b = super::body_cursor::char_byte_index(&self.param_edit_buf, self.param_cursor_char);
        let ch = self.param_edit_buf[b..].chars().next().unwrap();
        self.param_edit_buf.drain(b..b + ch.len_utf8());
    }

    pub fn request_sheet_row_count(&self) -> usize {
        let Some(api) = &self.current_api else {
            return 0;
        };
        let mut n = api.path_params.len()
            + api.query_params.len()
            + self.extra_query_params.len()
            + self.method_detail_headers.len()
            + self.extra_request_headers.len()
            + api.cookie_params.len()
            + api.headers.len();
        if api.body_binding.is_some() {
            n += 1;
        }
        n
    }

    #[allow(dead_code)]
    pub fn toggle_request_editor(&mut self) -> Result<(), &'static str> {
        if self.request_editor_open {
            self.save_request_editor_row();
            self.request_editor_error = None;
            self.persist_current_request_draft();
            self.request_editor_open = false;
            return Ok(());
        }
        self.request_editor_error = None;
        let Some(api) = self.current_api.as_ref() else {
            return Err("请先选择接口");
        };
        let n = self.request_sheet_row_count();
        if n == 0 {
            self.extra_query_params.push((String::new(), String::new()));
            self.request_editor_open = true;
            let pn = api.path_params.len();
            let qn = api.query_params.len();
            self.request_editor_focus = pn + qn;
            self.request_editor_extra_kv = true;
            self.load_request_editor_row();
            return Ok(());
        }
        self.request_editor_open = true;
        self.request_editor_focus = if api.body_binding.is_some() { n - 1 } else { 0 };
        self.request_editor_extra_kv = false;
        self.load_request_editor_row();
        Ok(())
    }

    pub fn move_request_editor_focus(&mut self, delta: isize) {
        let Some((start, end)) = self.request_editor_bounds() else {
            return;
        };
        self.save_request_editor_row();
        self.request_editor_extra_kv = false;
        let span = end.saturating_sub(start);
        if span == 0 {
            return;
        }
        let cur = self.request_editor_focus.clamp(start, end - 1) - start;
        let next = ((cur as isize + delta).rem_euclid(span as isize)) as usize;
        self.request_editor_focus = start + next;
        self.load_request_editor_row();
    }

    pub fn request_sheet_header_text(&self) -> String {
        let Some(_) = &self.current_api else {
            return String::new();
        };
        match self.detail_pane {
            DetailPane::Params => {
                "可编辑：Path / Query / 附加 Query · Enter 换行 · Tab 切换 · + 新参数 · - 删除参数"
                    .into()
            }
            DetailPane::Headers => {
                "可编辑：预设 Header / 附加 Header / Cookie / @RequestHeader · Enter 换行 · Tab 切换 · + 新参数 · - 删除参数".into()
            }
            DetailPane::Body => "仅编辑 Request Body JSON · 支持粘贴 · - 清空 · Esc 校验并退出"
                .into(),
        }
    }

    /// 请求表单内「当前正在编辑哪一项」单行提示（无列表，打开即可打字）。
    pub fn request_editor_focus_label_line(&self) -> String {
        let Some(api) = &self.current_api else {
            return String::new();
        };
        let Some((start, end)) = self.request_editor_bounds() else {
            return String::new();
        };
        let total = end.saturating_sub(start);
        if total == 0 {
            return String::new();
        }
        let i = self.request_editor_focus.clamp(start, end - 1) - start + 1;
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let en = self.extra_query_params.len();
        let hn = self.method_detail_headers.len();
        let xhn = self.extra_request_headers.len();
        let cn = api.cookie_params.len();
        let rhn = api.headers.len();
        if f < pn {
            let p = &api.path_params[f];
            return format!("Path · {}    [{i}/{total}]", p.name);
        }
        if f < pn + qn {
            let p = &api.query_params[f - pn];
            return format!("Query · {}    [{i}/{total}]", p.name);
        }
        if f < pn + qn + en {
            let j = f - pn - qn;
            let (ref kn, _) = self.extra_query_params[j];
            let part = if self.request_editor_extra_kv {
                "参数名"
            } else {
                "值"
            };
            let show = if kn.is_empty() {
                "（新参数）"
            } else {
                kn.as_str()
            };
            return format!("附加 Query · `{}` · {}    [{i}/{total}]", show, part);
        }
        if f < pn + qn + en + hn {
            let (hname, _) = &self.method_detail_headers[f - pn - qn - en];
            return format!("Header · {}    [{i}/{total}]", hname);
        }
        if f < pn + qn + en + hn + xhn {
            let j = f - pn - qn - en - hn;
            let (ref kn, _) = self.extra_request_headers[j];
            let part = if self.request_editor_extra_kv {
                "名称"
            } else {
                "值"
            };
            return format!("附加 Header · `{}` · {}    [{i}/{total}]", kn, part);
        }
        if f < pn + qn + en + hn + xhn + cn {
            let p = &api.cookie_params[f - pn - qn - en - hn - xhn];
            return format!("Cookie · {}    [{i}/{total}]", p.name);
        }
        if f < pn + qn + en + hn + xhn + cn + rhn {
            let p = &api.headers[f - pn - qn - en - hn - xhn - cn];
            return format!("Header · {}    [{i}/{total}]", p.name);
        }
        if let Some(b) = &api.body_binding {
            return format!(
                "当前 Body  ·  {}  ({})  · 下方为 JSON    [{i}/{total}]",
                b.name, b.java_type
            );
        }
        String::new()
    }

    #[allow(dead_code)]
    pub fn detail_lines(&self) -> String {
        if let Some(ix) = self.list_state.selected() {
            if let Some(ListRow::ClassHeader {
                scope_bucket,
                module,
                label,
            }) = self.list_rows.get(ix)
            {
                match scope_bucket {
                    None => {
                        let collapsed = self.collapsed_modules.contains(module);
                        let head = if label == module {
                            format!("类: {label}")
                        } else {
                            format!("类: {label}\nJava 类: {module}")
                        };
                        return if collapsed {
                            format!("{head}\n下属接口已隐藏。按 Space 或 Enter 展开。")
                        } else {
                            format!("{head}\n下属接口在列表中显示。按 Space 或 Enter 折叠隐藏。")
                        };
                    }
                    Some(b) => {
                        let collapsed = self
                            .collapsed_class_in_project
                            .contains(&(b.clone(), module.clone()));
                        let head = if label == module {
                            format!("项目: {b}\n类: {label}")
                        } else {
                            format!("项目: {b}\n类: {label}\nJava 类: {module}")
                        };
                        return if collapsed {
                            format!("{head}\n该类下接口已隐藏。按 Space 或 Enter 展开。")
                        } else {
                            format!("{head}\n该类下接口已列出。按 Space 或 Enter 折叠隐藏。")
                        };
                    }
                }
            }
            if let Some(ListRow::ProjectHeader(bucket)) = self.list_rows.get(ix) {
                let collapsed = self.collapsed_project_buckets.contains(bucket);
                let head = format!("项目目录（首级）: {bucket}");
                return if collapsed {
                    format!("{head}\n该目录下接口已隐藏。按 Space 或 Enter 展开。")
                } else {
                    format!("{head}\n该目录下接口已列出。按 Space 或 Enter 折叠隐藏。")
                };
            }
        }
        let Some(api) = &self.current_api else {
            return "← 从列表选择分组标题（Space/Enter 折叠）或一接口".into();
        };
        let mut s = String::new();
        s.push_str(&format!("{}  {}\n", api.http_method, api.path));
        s.push_str(&format!("{}\n", api.name));
        if api.project_bucket != "." && !api.project_bucket.is_empty() {
            s.push_str(&format!("项目目录: {}\n", api.project_bucket));
        }
        if let Some(doc) = &api.class_doc {
            if !doc.is_empty() {
                s.push_str(&format!("类注释: {doc}\n"));
            }
        }
        if let Some(tag) = &api.openapi_tag {
            if !tag.is_empty() {
                s.push_str(&format!("OpenAPI 标签: {tag}\n"));
            }
        }
        if let Some(doc) = &api.description {
            if !doc.is_empty() {
                s.push_str(&format!("接口说明: {doc}\n"));
            }
        }
        s.push_str(&format!("{}:{}\n", api.source_file, api.line));
        s.push_str(
            "按 e 修改请求参数（Path / Query / 附加 Query / 方法预设请求头 / Cookie / @RequestHeader / Body 同一窗口；Esc 保存退出；+ 增加附加 Query；附加行上 d 删除）\n\
以下为按 HTTP 方法与是否带 Body 预设的头（如 Accept、Content-Type）、Cookie 与扫描到的 `@RequestHeader`；全局附加头按 a（先合并 a，再合并本区，最后 `@RequestHeader` 同名覆盖；Cookie 合并进 `Cookie` 头）\n\n",
        );
        let path_n = api.path_params.len();
        let mh = self.method_detail_headers.len();
        let exn = self.extra_query_params.len();
        let cookie_n = api.cookie_params.len();
        let req_hn = api.headers.len();
        let n_params = path_n + api.query_params.len() + exn + mh + cookie_n + req_hn;
        if n_params > 0 {
            s.push_str("[参数]  [ / ] 详情内切换 Path → Query → 附加 Query → 预设请求头 → Cookie → @RequestHeader · e 修改请求参数\n");
        }
        if !api.path_params.is_empty() {
            s.push_str("[Path]\n");
            for (i, p) in api.path_params.iter().enumerate() {
                let mark = if self.param_focus == Some(i) {
                    "▶"
                } else {
                    " "
                };
                let v = self.path_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!(
                    " {}  {} : {} = {:?}\n",
                    mark, p.name, p.java_type, v
                ));
            }
            s.push('\n');
        }
        if !api.query_params.is_empty() {
            s.push_str("[Query]\n");
            for (j, p) in api.query_params.iter().enumerate() {
                let flat = path_n + j;
                let mark = if self.param_focus == Some(flat) {
                    "▶"
                } else {
                    " "
                };
                let v = self.query_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!(
                    " {}  {} : {} = {:?}\n",
                    mark, p.name, p.java_type, v
                ));
            }
            s.push('\n');
        }
        if !self.extra_query_params.is_empty() {
            s.push_str("[附加 Query]  （手动追加，参与预览与发送；同名覆盖扫描 Query）\n");
            for (k, (ek, ev)) in self.extra_query_params.iter().enumerate() {
                let flat = path_n + api.query_params.len() + k;
                let mark = if self.param_focus == Some(flat) {
                    "▶"
                } else {
                    " "
                };
                let label = if ek.trim().is_empty() {
                    "（空参数名不参与 URL）"
                } else {
                    ek.as_str()
                };
                s.push_str(&format!(" {}  {} = {:?}\n", mark, label, ev));
            }
            s.push('\n');
        }
        if !self.method_detail_headers.is_empty() {
            s.push_str(&format!(
                "[预设 Header]  （按 {} 与是否带 Body 预设 Accept / Content-Type 等，可改）\n",
                api.http_method
            ));
            for (k, (hname, hval)) in self.method_detail_headers.iter().enumerate() {
                let flat = path_n + api.query_params.len() + exn + k;
                let mark = if self.param_focus == Some(flat) {
                    "▶"
                } else {
                    " "
                };
                s.push_str(&format!(" {}  {} = {:?}\n", mark, hname, hval));
            }
            s.push('\n');
        }
        if !api.cookie_params.is_empty() {
            s.push_str(
                "[Cookie]  （由 `@CookieValue` 扫描得到，可改；发送时合并到 `Cookie` 头）\n",
            );
            for (k, p) in api.cookie_params.iter().enumerate() {
                let flat = path_n + api.query_params.len() + exn + mh + k;
                let mark = if self.param_focus == Some(flat) {
                    "▶"
                } else {
                    " "
                };
                let v = self.cookie_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!(
                    " {}  {} : {} = {:?}\n",
                    mark, p.name, p.java_type, v
                ));
            }
            s.push('\n');
        }
        if !api.headers.is_empty() {
            s.push_str("[@RequestHeader]  （由扫描得到，可改；发送时覆盖同名全局头/预设头）\n");
            for (k, p) in api.headers.iter().enumerate() {
                let flat = path_n + api.query_params.len() + exn + mh + cookie_n + k;
                let mark = if self.param_focus == Some(flat) {
                    "▶"
                } else {
                    " "
                };
                let v = self.header_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!(
                    " {}  {} : {} = {:?}\n",
                    mark, p.name, p.java_type, v
                ));
            }
            s.push('\n');
        }
        if let Some(b) = &api.body_binding {
            s.push_str(&format!("[Body] {} {} \n", b.name, b.java_type));
        }
        if api.body_binding.is_some() {
            if api.body.is_none() {
                s.push_str("(Body JSON 等待源码索引或无法解析 DTO，片刻后重试或检查类型名)\n\n");
            } else {
                s.push_str(&format!("{}\n\n", self.body_draft));
            }
        }
        let preview = crate::http_exec::compose_url(
            self.active_base_url(),
            &api.path,
            &self.path_vals,
            &self.merged_query_for_url(),
        );
        match preview {
            Ok(u) => s.push_str(&format!("\n预览 URL:\n{u}\n")),
            Err(e) => s.push_str(&format!("\n预览 URL 失败: {e}\n")),
        }
        s
    }

    pub fn clone_list_items(&self) -> Vec<(String, Style)> {
        use ratatui::style::Color;
        self.list_rows
            .iter()
            .map(|row| match row {
                ListRow::ClassHeader {
                    scope_bucket,
                    module,
                    label,
                } => {
                    let (collapsed, indent) = match scope_bucket {
                        None => (self.collapsed_modules.contains(module), ""),
                        Some(b) => (
                            self.collapsed_class_in_project
                                .contains(&(b.clone(), module.clone())),
                            "  ",
                        ),
                    };
                    let chevron = if collapsed { "▶" } else { "▼" };
                    (
                        format!("{indent}{chevron} {label}"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                }
                ListRow::ProjectHeader(bucket) => (
                    if self.collapsed_project_buckets.contains(bucket) {
                        format!("▶ 📁 {bucket}")
                    } else {
                        format!("▼ 📁 {bucket}")
                    },
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                ListRow::Endpoint { api_index } => {
                    let a = &self.apis[*api_index];
                    let style = method_style(&a.http_method);
                    let suffix = a
                        .description
                        .as_deref()
                        .map(|d| {
                            let short: String = d.chars().take(20).collect();
                            let ell = if d.chars().count() > 20 { "…" } else { "" };
                            format!("  · {short}{ell}")
                        })
                        .unwrap_or_default();
                    let indent = match self.list_group_mode {
                        ListGroupMode::ByProjectFolder => "      ",
                        _ => "   ",
                    };
                    let line = format!("{indent}{:7} {}{suffix}", a.http_method, a.path);
                    (line, style)
                }
            })
            .collect()
    }

    pub fn list_title_suffix(&self) -> &'static str {
        match self.list_group_mode {
            ListGroupMode::ByController => " [按类 · Space/Enter]",
            ListGroupMode::ByProjectFolder => " [按目录 · Space/Enter 项目/类]",
            ListGroupMode::Flat => " [平铺]",
        }
    }
}

fn method_style(method: &str) -> Style {
    use ratatui::style::{Color, Style};
    match method.to_uppercase().as_str() {
        "GET" => Style::default().fg(Color::Green),
        "POST" => Style::default().fg(Color::Cyan),
        "PUT" => Style::default().fg(Color::Yellow),
        "DELETE" => Style::default().fg(Color::Red),
        "PATCH" => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::White),
    }
}

fn object_summary(len: usize) -> String {
    format!("{{{len} 项}}")
}

fn array_summary(len: usize) -> String {
    format!("[{len} 项]")
}

fn scalar_summary(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn escape_json_pointer(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiParam, LocalApi, ParamLocation};

    fn req_param(name: &str, location: ParamLocation, required: bool) -> ApiParam {
        ApiParam {
            name: name.into(),
            java_type: "String".into(),
            location,
            required,
            default_value: None,
        }
    }

    fn make_app_with_api(api: LocalApi) -> App {
        let mut app = App::new(PathBuf::from("."));
        app.restore_defaults_for_api(&api);
        app.current_api = Some(api);
        app
    }

    #[test]
    fn request_editor_includes_scanned_request_headers() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.echo".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/echo".into(),
            ".".into(),
        );
        api.headers
            .push(req_param("X-Tenant", ParamLocation::Header, true));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let app = make_app_with_api(api);
        assert_eq!(app.request_sheet_row_count(), 4);
    }

    #[test]
    fn validate_request_blocks_missing_required_header() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );
        api.headers
            .push(req_param("Authorization", ParamLocation::Header, true));

        let app = make_app_with_api(api);
        assert!(app
            .validate_request_before_send()
            .unwrap_err()
            .contains("Header `Authorization`"));
    }

    #[test]
    fn merged_http_headers_contains_cookie_header_from_cookie_params() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );
        api.cookie_params
            .push(req_param("sid", ParamLocation::Cookie, true));
        api.cookie_params
            .push(req_param("tenant", ParamLocation::Cookie, false));

        let mut app = make_app_with_api(api);
        app.cookie_vals.insert("sid".into(), "abc".into());
        app.cookie_vals.insert("tenant".into(), "t1".into());

        let headers = app.merged_http_headers();
        let cookie = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("cookie"))
            .map(|(_, v)| v.as_str());
        assert_eq!(cookie, Some("sid=abc; tenant=t1"));
    }

    #[test]
    fn merged_http_headers_contains_extra_request_header() {
        let api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );

        let mut app = make_app_with_api(api);
        app.extra_request_headers
            .push(("X-Trace-Id".into(), "trace-1".into()));

        let headers = app.merged_http_headers();
        let trace = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-trace-id"))
            .map(|(_, v)| v.as_str());
        assert_eq!(trace, Some("trace-1"));
    }

    #[test]
    fn validate_request_blocks_missing_required_cookie() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );
        api.cookie_params
            .push(req_param("sid", ParamLocation::Cookie, true));

        let app = make_app_with_api(api);
        assert!(app
            .validate_request_before_send()
            .unwrap_err()
            .contains("Cookie `sid`"));
    }

    #[test]
    fn toggle_request_editor_prefers_body_row_when_body_exists() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.headers
            .push(req_param("X-Tenant", ParamLocation::Header, false));
        api.cookie_params
            .push(req_param("sid", ParamLocation::Cookie, false));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));
        api.body = Some(serde_json::json!({"name": null}));

        let mut app = make_app_with_api(api);
        app.toggle_request_editor().unwrap();

        assert!(app.request_editor_is_body_row());
        assert!(app.body_buf.contains("name"));
    }

    #[test]
    fn select_main_module_digit_maps_detail_shortcuts() {
        let api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );
        let mut app = make_app_with_api(api);

        assert!(app.select_main_module_digit('4'));
        assert_eq!(app.main_panel, MainPanel::Detail);
        assert_eq!(app.detail_pane, DetailPane::Params);

        assert!(app.select_main_module_digit('5'));
        assert_eq!(app.main_panel, MainPanel::Detail);
        assert_eq!(app.detail_pane, DetailPane::Headers);

        assert!(app.select_main_module_digit('6'));
        assert_eq!(app.main_panel, MainPanel::Detail);
        assert_eq!(app.detail_pane, DetailPane::Body);

        assert!(app.select_main_module_digit('7'));
        assert_eq!(app.main_panel, MainPanel::Response);
    }

    #[test]
    fn request_editor_focus_stays_within_headers_mode() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.path_params
            .push(req_param("id", ParamLocation::Path, true));
        api.headers
            .push(req_param("X-Tenant", ParamLocation::Header, false));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.detail_pane = DetailPane::Headers;
        app.open_request_editor_for_detail_pane().unwrap();
        let (_, headers_end) = app.request_editor_bounds().unwrap();

        app.move_request_editor_focus(1);

        assert!(app.request_editor_focus < headers_end);
        assert!(!app.request_editor_is_body_row());
    }

    #[test]
    fn deleting_last_extra_param_stays_in_params_mode() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.headers
            .push(req_param("X-Tenant", ParamLocation::Header, false));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.detail_pane = DetailPane::Params;
        app.extra_query_params.push(("q".into(), "1".into()));
        app.open_request_editor_for_detail_pane().unwrap();

        assert!(app.delete_extra_query_row_at_focus());
        assert_eq!(app.detail_pane, DetailPane::Params);
        assert_eq!(app.extra_query_params.len(), 1);
        assert!(app.request_editor_bounds().is_some());
        assert!(!app.request_editor_is_body_row());
    }

    #[test]
    fn validate_body_editor_json_rejects_invalid_json() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.body_buf = "{\"name\":".into();

        assert!(app.validate_body_editor_json().is_err());
    }

    #[test]
    fn validate_body_editor_json_allows_empty_body() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let app = make_app_with_api(api);

        assert!(app.validate_body_editor_json().is_ok());
    }

    #[test]
    fn switching_between_apis_preserves_request_edits() {
        let mut api1 = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api1.path_params
            .push(req_param("id", ParamLocation::Path, true));
        api1.query_params
            .push(req_param("q", ParamLocation::Query, false));
        api1.cookie_params
            .push(req_param("sid", ParamLocation::Cookie, false));
        api1.headers
            .push(req_param("X-Tenant", ParamLocation::Header, false));
        api1.body_binding = Some(req_param("body", ParamLocation::Body, true));
        api1.body = Some(serde_json::json!({"name": null}));

        let api2 = LocalApi::new_stub(
            "2".into(),
            "Demo.other".into(),
            "DemoController".into(),
            "Demo.java".into(),
            2,
            "GET".into(),
            "/other".into(),
            ".".into(),
        );

        let mut app = App::new(PathBuf::from("."));
        app.apis = vec![api1.clone(), api2];
        app.list_rows = vec![ListRow::Endpoint { api_index: 0 }, ListRow::Endpoint { api_index: 1 }];

        app.list_state.select(Some(0));
        app.sync_detail_from_selection();
        app.path_vals.insert("id".into(), "42".into());
        app.query_vals.insert("q".into(), "search".into());
        app.extra_query_params
            .push(("page".into(), "2".into()));
        app.method_detail_headers[0].1 = "application/xml".into();
        app.extra_request_headers
            .push(("X-Trace-Id".into(), "trace-1".into()));
        app.header_vals.insert("X-Tenant".into(), "tenant-a".into());
        app.cookie_vals.insert("sid".into(), "cookie-a".into());
        app.body_draft = "{\"name\":\"alice\"}".into();

        app.list_state.select(Some(1));
        app.sync_detail_from_selection();

        app.list_state.select(Some(0));
        app.sync_detail_from_selection();

        assert_eq!(app.path_vals.get("id").map(String::as_str), Some("42"));
        assert_eq!(app.query_vals.get("q").map(String::as_str), Some("search"));
        assert_eq!(
            app.extra_query_params,
            vec![("page".into(), "2".into())]
        );
        assert_eq!(
            app.method_detail_headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str()),
            Some("application/xml")
        );
        assert_eq!(
            app.extra_request_headers,
            vec![("X-Trace-Id".into(), "trace-1".into())]
        );
        assert_eq!(
            app.header_vals.get("X-Tenant").map(String::as_str),
            Some("tenant-a")
        );
        assert_eq!(app.cookie_vals.get("sid").map(String::as_str), Some("cookie-a"));
        assert_eq!(app.body_draft, "{\"name\":\"alice\"}");
    }

    #[test]
    fn selecting_module_digit_closes_transient_panels_and_editor() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.hosts_panel_open = true;
        app.hosts_edit_line = true;
        app.headers_panel_open = true;
        app.headers_edit_line = true;
        app.search_focus = true;
        app.request_editor_open = true;
        app.body_buf = "{\"name\":\"alice\"}".into();
        app.request_editor_focus = app.request_sheet_row_count().saturating_sub(1);

        assert!(app.select_main_module_digit('4'));

        assert!(!app.hosts_panel_open);
        assert!(!app.hosts_edit_line);
        assert!(!app.headers_panel_open);
        assert!(!app.headers_edit_line);
        assert!(!app.search_focus);
        assert!(!app.request_editor_open);
        assert_eq!(app.body_draft, "{\"name\":\"alice\"}");
        assert_eq!(app.main_panel, MainPanel::Detail);
        assert_eq!(app.detail_pane, DetailPane::Params);
    }

    #[test]
    fn sync_detail_keeps_request_editor_open_when_selection_is_unchanged() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = App::new(PathBuf::from("."));
        app.apis = vec![api];
        app.list_rows = vec![ListRow::Endpoint { api_index: 0 }];
        app.list_state.select(Some(0));
        app.sync_detail_from_selection();
        app.detail_pane = DetailPane::Body;
        app.open_request_editor_for_detail_pane().unwrap();

        assert!(app.request_editor_open);
        app.sync_detail_from_selection();
        assert!(app.request_editor_open);
        assert!(app.request_editor_is_body_row());
    }

    #[test]
    fn edit_detail_auto_selects_first_endpoint_when_none_is_selected() {
        let api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );

        let mut app = App::new(PathBuf::from("."));
        app.apis = vec![api];
        app.filtered = vec![0];
        app.collapsed_project_buckets.insert(".".into());
        app.rebuild_list_rows();
        app.list_state.select(Some(0));
        app.main_panel = MainPanel::Detail;
        app.detail_pane = DetailPane::Params;

        assert!(app.edit_current_module().is_ok());
        assert!(app.current_api.is_some());
        assert!(app.request_editor_open);
        assert!(app.status_msg.contains("已自动展开并选中首个接口"));
    }

    #[test]
    fn persist_current_request_draft_saves_current_api_state_into_cache() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.path_params
            .push(req_param("id", ParamLocation::Path, true));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.path_vals.insert("id".into(), "99".into());
        app.body_draft = "{\"name\":\"persisted\"}".into();

        app.persist_current_request_draft();

        let draft = app
            .request_drafts
            .get(&app.current_api.as_ref().unwrap().request_draft_key())
            .expect("draft should exist");
        assert_eq!(draft.path_vals.get("id").map(String::as_str), Some("99"));
        assert_eq!(draft.body_draft, "{\"name\":\"persisted\"}");
    }

    #[test]
    fn prepare_request_for_send_builds_url_headers_and_body() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo/{id}".into(),
            ".".into(),
        );
        api.path_params
            .push(req_param("id", ParamLocation::Path, true));
        api.query_params
            .push(req_param("q", ParamLocation::Query, false));
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.path_vals.insert("id".into(), "42".into());
        app.query_vals.insert("q".into(), "abc".into());
        app.body_draft = "{\"ok\":true}".into();
        app.request_headers.push(HeaderEntry {
            name: "Authorization".into(),
            value: "Bearer token".into(),
            description: None,
        });

        let (method, url, headers, body) = app.prepare_request_for_send().unwrap();

        assert_eq!(method, "POST");
        assert_eq!(url, format!("{}/demo/42?q=abc", app.active_base_url()));
        assert_eq!(body.as_deref(), Some("{\"ok\":true}"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer token"));
    }

    #[test]
    fn prepare_request_for_send_blocks_missing_required_values() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo/{id}".into(),
            ".".into(),
        );
        api.path_params
            .push(req_param("id", ParamLocation::Path, true));

        let mut app = make_app_with_api(api);
        app.path_vals.insert("id".into(), String::new());

        assert!(app.prepare_request_for_send().is_err());
    }

    #[test]
    fn response_json_lines_show_only_first_level_by_default() {
        let mut app = App::new(PathBuf::from("."));
        app.set_last_http_response(HttpResult {
            status: 200,
            elapsed_ms: 12,
            headers_text: "content-type: application/json".into(),
            body: "{\"user\":{\"name\":\"alice\"},\"ok\":true}".into(),
        });

        let lines = app.response_json_lines();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].label, "ok: true");
        assert_eq!(lines[1].label, "user: {1 项}");
        assert!(lines[1].expandable);
        assert!(!lines[1].expanded);
    }

    #[test]
    fn response_json_toggle_expands_selected_node() {
        let mut app = App::new(PathBuf::from("."));
        app.set_last_http_response(HttpResult {
            status: 200,
            elapsed_ms: 12,
            headers_text: "content-type: application/json".into(),
            body: "{\"user\":{\"name\":\"alice\"},\"ok\":true}".into(),
        });
        app.response_focus = 1;

        assert!(app.toggle_response_node_at_focus());
        let lines = app.response_json_lines();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2].label, "name: \"alice\"");
    }

    #[test]
    fn ensure_response_focus_visible_scrolls_with_keyboard_focus() {
        let mut app = App::new(PathBuf::from("."));
        app.set_last_http_response(HttpResult {
            status: 200,
            elapsed_ms: 12,
            headers_text: String::new(),
            body: "{\"a\":1,\"b\":2,\"c\":3,\"d\":4}".into(),
        });

        app.move_response_focus(3);
        app.ensure_response_focus_visible(3);

        assert_eq!(app.response_focus, 3);
        assert_eq!(app.response_scroll, 3);
    }

}
