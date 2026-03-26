//! TUI 应用状态。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::ListState;

use crate::model::LocalApi;
use crate::scanner::{JavaSourceCache, ScanReport};
use crate::http_exec::HttpResult;
use crate::user_config::{HeaderEntry, HostEntry};

/// 左侧列表分组方式（`g` 循环切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    Endpoint { api_index: usize },
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
/// 主界面五大区块焦点（数字键 1–5、鼠标左键切换）。
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

impl MainPanel {
    pub fn from_digit(c: char) -> Option<Self> {
        Some(match c {
            '1' => Self::ApiList,
            '2' => Self::HostSidebar,
            '3' => Self::HeaderSidebar,
            '4' => Self::Detail,
            '5' => Self::Response,
            _ => return None,
        })
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
    pub detail_inner: Rect,
    pub response: Rect,
    pub response_inner: Rect,
}

fn default_method_detail_headers(_http_method: &str, has_request_body: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if has_request_body {
        out.push((
            "Content-Type".to_string(),
            "application/json".to_string(),
        ));
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
    /// 详情 / `e` 表单：按当前方法（GET/POST…）与是否有 Body 预设的请求头，可改值（顺序即发送时相对顺序，不含全局 `a`）。
    pub method_detail_headers: Vec<(String, String)>,
    /// 扫描到的 `@RequestHeader`：仅参与发送合并，不在详情里展示编辑。
    pub header_vals: HashMap<String, String>,
    pub body_draft: String,
    pub last_http: Option<HttpResult>,
    pub pending_request: bool,
    pub status_msg: String,
    pub search_focus: bool,
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
    /// 表单内扁平焦点：先全部 Path，再 Query，最后一行 Body（若有）。
    pub request_editor_focus: usize,
    /// 主界面当前焦点区块（1–5 / 鼠标切换）。
    pub main_panel: MainPanel,
    /// 最近一次绘制时的主界面布局（供鼠标命中）。
    pub last_main_layout: Option<MainUiLayout>,
    /// 详情 Paragraph 垂直滚动行数。
    pub detail_scroll: u16,
    /// 响应区 Paragraph 垂直滚动行数。
    pub response_scroll: u16,
    pub host_sidebar_scroll: u16,
    pub header_sidebar_scroll: u16,
}

impl App {
    pub fn new(project: PathBuf) -> Self {
        let cfg = crate::user_config::load().unwrap_or_else(|_| crate::user_config::UserConfig::default());
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
        let base_url_index = cfg
            .selected_base_url
            .min(hosts.len().saturating_sub(1));
        let request_headers = cfg.request_headers;
        let selected_request_header = if request_headers.is_empty() {
            0
        } else {
            cfg.selected_request_header.min(request_headers.len() - 1)
        };

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
            method_detail_headers: Vec::new(),
            header_vals: HashMap::new(),
            body_draft: String::new(),
            last_http: None,
            pending_request: false,
            status_msg: String::new(),
            search_focus: false,
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
            main_panel: MainPanel::default(),
            last_main_layout: None,
            detail_scroll: 0,
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

    pub fn persist_config(&mut self) {
        let mut cfg = crate::user_config::load().unwrap_or_default();
        cfg.hosts = self.hosts.clone();
        cfg.selected_base_url = self.base_url_index;
        cfg.request_headers = self.request_headers.clone();
        cfg.selected_request_header = self.selected_request_header;
        if let Err(e) = crate::user_config::save(&cfg) {
            self.status_msg = format!("保存配置失败: {e}");
        }
    }

    pub fn open_hosts_panel(&mut self) {
        self.hosts_panel_open = true;
        self.hosts_edit_line = false;
        self.hosts_edit_kind = None;
        let i = self
            .base_url_index
            .min(self.hosts.len().saturating_sub(1));
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
            self.base_url_index = self
                .base_url_index
                .min(self.hosts.len().saturating_sub(1));
        }
        let max = self.hosts.len().saturating_sub(1);
        let sel = self
            .hosts_list_state
            .selected()
            .unwrap_or(0)
            .min(max);
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
    pub fn extra_headers_for_http(&self) -> Vec<(String, String)> {
        self.request_headers
            .iter()
            .filter(|h| !h.name.trim().is_empty())
            .map(|h| (h.name.trim().to_string(), h.value.clone()))
            .collect()
    }

    /// 合并顺序：全局头（`a`）→ 详情按方法预设头 → 扫描到的 `@RequestHeader`；后者同名覆盖前者。
    /// 空串或全空白值不会加入请求。
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
                acc.insert(
                    p.name.to_ascii_lowercase(),
                    (p.name.clone(), v.to_string()),
                );
            }
        }
        acc.into_values().collect()
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
            self.header_buf = self.request_headers[i].description.clone().unwrap_or_default();
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
                    self.collapsed_modules = idxs
                        .iter()
                        .map(|&i| self.apis[i].module.clone())
                        .collect();
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
                        self.list_rows
                            .push(ListRow::ProjectHeader(b.to_string()));
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
            ListGroupMode::ByProjectFolder => {
                match self.list_rows.get(ix) {
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
                }
            }
        }
        true
    }

    pub fn sync_detail_from_selection(&mut self) {
        let Some(row_ix) = self.list_state.selected() else {
            self.current_api = None;
            return;
        };
        let api_ix = match self.list_rows.get(row_ix) {
            Some(ListRow::Endpoint { api_index }) => *api_index,
            Some(ListRow::ClassHeader { .. }) | Some(ListRow::ProjectHeader(_)) => {
                self.current_api = None;
                return;
            }
            None => {
                self.current_api = None;
                return;
            }
        };
        let mut lazy_filled = false;
        if let Some(cache) = self.source_cache.clone() {
            lazy_filled =
                crate::scanner::ensure_request_body_resolved(&mut self.apis[api_ix], &cache, &self.project);
        }
        let api = self.apis[api_ix].clone();
        let id_changed = self.current_api.as_ref().map(|a| a.id != api.id).unwrap_or(true);
        if id_changed {
            self.param_focus = None;
            self.detail_scroll = 0;
            if self.request_editor_open {
                self.save_request_editor_row();
                self.request_editor_open = false;
            }
            self.restore_defaults_for_api(&api);
            self.current_api = Some(api);
        } else if lazy_filled {
            self.detail_scroll = 0;
            self.restore_defaults_for_api(&api);
            self.current_api = Some(api);
        }
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
            self.query_vals.insert(
                p.name.clone(),
                p.default_value.clone().unwrap_or_default(),
            );
        }
        self.method_detail_headers =
            default_method_detail_headers(&api.http_method, api.body_binding.is_some());
        self.header_vals.clear();
        for p in &api.headers {
            self.header_vals.insert(
                p.name.clone(),
                p.default_value.clone().unwrap_or_default(),
            );
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

    fn param_flat_len(&self) -> usize {
        let Some(api) = &self.current_api else {
            return 0;
        };
        api.path_params.len()
            + api.query_params.len()
            + self.method_detail_headers.len()
    }

    pub fn move_param_focus(&mut self, delta: isize) {
        let n = self.param_flat_len();
        if n == 0 {
            return;
        }
        let n_i = n as isize;
        let cur = match self.param_focus {
            Some(i) => i as isize,
            None => {
                if delta > 0 {
                    -1
                } else {
                    n_i
                }
            }
        };
        let next = (cur + delta).rem_euclid(n_i) as usize;
        self.param_focus = Some(next);
    }

    pub fn request_editor_is_body_row(&self) -> bool {
        let Some(api) = &self.current_api else {
            return false;
        };
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let hn = self.method_detail_headers.len();
        api.body_binding.is_some() && self.request_editor_focus >= pn + qn + hn
    }

    pub fn save_request_editor_row(&mut self) {
        let Some(api) = &self.current_api else {
            return;
        };
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let hn = self.method_detail_headers.len();
        if f < pn {
            let name = api.path_params[f].name.clone();
            self.path_vals.insert(name, self.param_edit_buf.clone());
        } else if f < pn + qn {
            let name = api.query_params[f - pn].name.clone();
            self.query_vals.insert(name, self.param_edit_buf.clone());
        } else if f < pn + qn + hn {
            let row = f - pn - qn;
            if let Some(slot) = self.method_detail_headers.get_mut(row) {
                slot.1 = self.param_edit_buf.clone();
            }
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
        let hn = self.method_detail_headers.len();
        if f < pn {
            let name = &api.path_params[f].name;
            self.param_edit_buf = self.path_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn {
            let name = &api.query_params[f - pn].name;
            self.param_edit_buf = self.query_vals.get(name).cloned().unwrap_or_default();
            self.param_cursor_char = self.param_edit_buf.chars().count();
        } else if f < pn + qn + hn {
            let row = f - pn - qn;
            self.param_edit_buf = self
                .method_detail_headers
                .get(row)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
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
        let b = super::body_cursor::char_byte_index(&self.param_edit_buf, self.param_cursor_char - 1);
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
            + self.method_detail_headers.len();
        if api.body_binding.is_some() {
            n += 1;
        }
        n
    }

    pub fn toggle_request_editor(&mut self) -> Result<(), &'static str> {
        if self.request_editor_open {
            self.save_request_editor_row();
            self.request_editor_open = false;
            return Ok(());
        }
        let Some(_api) = &self.current_api else {
            return Err("请先选择接口");
        };
        let n = self.request_sheet_row_count();
        if n == 0 {
            return Err("当前接口无可编辑项");
        }
        self.request_editor_open = true;
        self.request_editor_focus = 0;
        self.load_request_editor_row();
        Ok(())
    }

    pub fn move_request_editor_focus(&mut self, delta: isize) {
        let n = self.request_sheet_row_count();
        if n == 0 {
            return;
        }
        self.save_request_editor_row();
        let n_i = n as isize;
        let cur = self.request_editor_focus as isize;
        self.request_editor_focus = ((cur + delta).rem_euclid(n_i)) as usize;
        self.load_request_editor_row();
    }

    pub fn request_sheet_header_text(&self) -> String {
        let Some(api) = &self.current_api else {
            return String::new();
        };
        format!(
            "{}  {}\n{}\n直接输入下方编辑区 · Esc 保存退出 · Tab 切换字段 · Path/Query/方法预设头 内 ←/→ · Body 内 ↑/↓/←/→ 移动光标",
            api.http_method,
            api.path,
            api.name
        )
    }

    /// 请求表单内「当前正在编辑哪一项」单行提示（无列表，打开即可打字）。
    pub fn request_editor_focus_label_line(&self) -> String {
        let Some(api) = &self.current_api else {
            return String::new();
        };
        let total = self.request_sheet_row_count();
        if total == 0 {
            return String::new();
        }
        let i = self.request_editor_focus + 1;
        let f = self.request_editor_focus;
        let pn = api.path_params.len();
        let qn = api.query_params.len();
        let hn = self.method_detail_headers.len();
        if f < pn {
            let p = &api.path_params[f];
            return format!(
                "当前 Path  ·  {}  ({})  = 编辑下方值    [{i}/{total}]",
                p.name, p.java_type
            );
        }
        if f < pn + qn {
            let p = &api.query_params[f - pn];
            return format!(
                "当前 Query ·  {}  ({})  = 编辑下方值    [{i}/{total}]",
                p.name, p.java_type
            );
        }
        if f < pn + qn + hn {
            let (hname, _) = &self.method_detail_headers[f - pn - qn];
            return format!(
                "当前 Header ·  {} （按方法预设） = 编辑下方值    [{i}/{total}]",
                hname
            );
        }
        if let Some(b) = &api.body_binding {
            return format!(
                "当前 Body  ·  {}  ({})  · 下方为 JSON    [{i}/{total}]",
                b.name, b.java_type
            );
        }
        String::new()
    }

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
            "按 e 修改请求参数（Path / Query / 方法预设请求头 / Body 同一窗口；Esc 保存退出，Tab 切换字段）\n\
以下为按 HTTP 方法与是否带 Body 预设的头（如 Accept、Content-Type），非注解扫描；全局附加头按 a（先合并 a，再合并本区，最后 @RequestHeader 同名覆盖）\n\n",
        );
        let path_n = api.path_params.len();
        let mh = self.method_detail_headers.len();
        let n_params = path_n + api.query_params.len() + mh;
        if n_params > 0 {
            s.push_str("[参数]  [ / ] 详情内切换 Path → Query → 请求头 · e 修改请求参数\n");
        }
        if !api.path_params.is_empty() {
            s.push_str("[Path]\n");
            for (i, p) in api.path_params.iter().enumerate() {
                let mark = if self.param_focus == Some(i) { "▶" } else { " " };
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
                let mark = if self.param_focus == Some(flat) { "▶" } else { " " };
                let v = self.query_vals.get(&p.name).cloned().unwrap_or_default();
                s.push_str(&format!(
                    " {}  {} : {} = {:?}\n",
                    mark, p.name, p.java_type, v
                ));
            }
            s.push('\n');
        }
        if !self.method_detail_headers.is_empty() {
            s.push_str(&format!(
                "[Header]  （按 {} 与是否带 Body 预设 Accept / Content-Type 等，可改）\n",
                api.http_method
            ));
            for (k, (hname, hval)) in self.method_detail_headers.iter().enumerate() {
                let flat = path_n + api.query_params.len() + k;
                let mark = if self.param_focus == Some(flat) { "▶" } else { " " };
                s.push_str(&format!(" {}  {} = {:?}\n", mark, hname, hval));
            }
            s.push('\n');
        }
        if !api.headers.is_empty() {
            s.push_str(&format!(
                "（另有 {} 个 @RequestHeader 由扫描预填并参与发送，不在此列表编辑）\n",
                api.headers.len()
            ));
        }
        if let Some(b) = &api.body_binding {
            s.push_str(&format!(
                "[Body] {} {} \n",
                b.name, b.java_type
            ));
        }
        if api.body_binding.is_some() {
            if api.body.is_none() {
                s.push_str("(Body JSON 等待源码索引或无法解析 DTO，片刻后重试或检查类型名)\n\n");
            } else {
                s.push_str(&format!("{}\n\n", self.body_draft));
            }
        }
        if !api.cookie_params.is_empty() {
            s.push_str("[Cookie] （已扫描，编辑与发送待后续版本）\n");
            for p in &api.cookie_params {
                s.push_str(&format!("     {} : {}\n", p.name, p.java_type));
            }
        }
        let preview = crate::http_exec::compose_url(
            self.active_base_url(),
            &api.path,
            &self.path_vals,
            &self.query_vals,
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
                    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
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
            ListGroupMode::ByController => " [按类 g·Space/Enter]",
            ListGroupMode::ByProjectFolder => " [按目录 g·Space/Enter 项目/类]",
            ListGroupMode::Flat => " [平铺 g]",
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
