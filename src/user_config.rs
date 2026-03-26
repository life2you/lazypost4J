//! 用户配置（域名列表、自定义请求头等），持久化到本机配置目录。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 一项可切换的服务根地址；界面主文案优先用 `description`，无则显示 `url`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostEntry {
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl HostEntry {
    /// 列表/侧栏主展示：有非空描述用描述，否则用 URL。
    pub fn display_label(&self) -> String {
        if let Some(d) = &self.description {
            let t = d.trim();
            if !t.is_empty() {
                return d.to_string();
            }
        }
        self.url.clone()
    }

    /// 当有描述时，用于副行展示真实 URL。
    pub fn display_subtitle(&self) -> Option<&str> {
        if let Some(d) = &self.description {
            let t = d.trim();
            if !t.is_empty() {
                return Some(self.url.as_str());
            }
        }
        None
    }
}

/// 发送请求时附加的 HTTP 头；主文案优先 `description`，无则显示 `name: value`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl HeaderEntry {
    pub fn display_label(&self) -> String {
        if let Some(d) = &self.description {
            let t = d.trim();
            if !t.is_empty() {
                return d.to_string();
            }
        }
        let nv = format!("{}: {}", self.name.trim(), self.value);
        let count = nv.chars().count();
        if count > 52 {
            format!("{}…", nv.chars().take(49).collect::<String>())
        } else {
            nv
        }
    }

    pub fn display_subtitle(&self) -> Option<String> {
        if let Some(d) = &self.description {
            let t = d.trim();
            if !t.is_empty() {
                return Some(format!("{}: {}", self.name.trim(), self.value));
            }
        }
        None
    }
}

/// 最近打开过的项目根路径（canonical 字符串），最新在前。
pub const MAX_RECENT_PROJECTS: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default = "default_hosts")]
    pub hosts: Vec<HostEntry>,
    #[serde(default)]
    pub selected_base_url: usize,
    /// 发送请求时附加的头（`name` 非空才会发出）。
    #[serde(default)]
    pub request_headers: Vec<HeaderEntry>,
    #[serde(default)]
    pub selected_request_header: usize,
    /// 仅用于读取旧版配置；写入前会清空，迁移到 `request_headers`。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_header: Option<String>,
    #[serde(default)]
    pub recent_projects: Vec<String>,
}

fn default_hosts() -> Vec<HostEntry> {
    vec![HostEntry {
        url: "http://localhost:8080".to_string(),
        description: None,
    }]
}

/// 旧版仅 `base_urls` 数组。
#[derive(Deserialize)]
struct LegacyUserConfig {
    #[serde(default)]
    base_urls: Vec<String>,
    #[serde(default)]
    selected_base_url: usize,
    #[serde(default)]
    auth_header: Option<String>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            hosts: default_hosts(),
            selected_base_url: 0,
            request_headers: Vec::new(),
            selected_request_header: 0,
            auth_header: None,
            recent_projects: Vec::new(),
        }
    }
}

fn normalize(mut c: UserConfig) -> UserConfig {
    if c.hosts.is_empty() {
        c.hosts = default_hosts();
    }
    for h in &mut c.hosts {
        if h.url.trim().is_empty() {
            h.url = "http://localhost:8080".to_string();
        }
    }
    if c.selected_base_url >= c.hosts.len() {
        c.selected_base_url = 0;
    }

    if c.request_headers.is_empty() {
        if let Some(ref a) = c.auth_header {
            let t = a.trim();
            if !t.is_empty() {
                c.request_headers.push(HeaderEntry {
                    name: "Authorization".into(),
                    value: a.clone(),
                    description: None,
                });
            }
        }
    }
    c.auth_header = None;

    if c.request_headers.is_empty() {
        c.selected_request_header = 0;
    } else if c.selected_request_header >= c.request_headers.len() {
        c.selected_request_header = c.request_headers.len() - 1;
    }

    c.recent_projects.retain(|p| !p.trim().is_empty());
    c.recent_projects.truncate(MAX_RECENT_PROJECTS);
    c
}

/// 将本次打开的项目根记入历史（去重、最新在前）。
pub fn record_recent_project(path: &std::path::Path) -> anyhow::Result<()> {
    let mut c = load().unwrap_or_default();
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = canon.display().to_string();
    c.recent_projects.retain(|p| p != &s);
    c.recent_projects.insert(0, s);
    c.recent_projects.truncate(MAX_RECENT_PROJECTS);
    save(&c)
}

/// 替换整个「最近项目」列表（用于在选目录界面删除某条）。
pub fn set_recent_projects(paths: Vec<String>) -> anyhow::Result<()> {
    let mut c = load().unwrap_or_default();
    c.recent_projects = paths;
    c.recent_projects.truncate(MAX_RECENT_PROJECTS);
    save(&c)
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("lazypost").join("config.json"))
}

pub fn load() -> anyhow::Result<UserConfig> {
    let Some(path) = config_path() else {
        return Ok(UserConfig::default());
    };
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let s = std::fs::read_to_string(&path)?;
    let v: serde_json::Value = serde_json::from_str(&s)?;
    if v.get("hosts").is_some() {
        let c: UserConfig = serde_json::from_value(v)?;
        return Ok(normalize(c));
    }
    if v.get("base_urls").is_some() {
        let leg: LegacyUserConfig = serde_json::from_value(v)?;
        let hosts = leg
            .base_urls
            .into_iter()
            .map(|url| HostEntry {
                url,
                description: None,
            })
            .collect::<Vec<_>>();
        let c = UserConfig {
            hosts: if hosts.is_empty() {
                default_hosts()
            } else {
                hosts
            },
            selected_base_url: leg.selected_base_url,
            request_headers: Vec::new(),
            selected_request_header: 0,
            auth_header: leg.auth_header,
            recent_projects: Vec::new(),
        };
        return Ok(normalize(c));
    }
    let c: UserConfig = serde_json::from_str(&s)?;
    Ok(normalize(c))
}

pub fn save(config: &UserConfig) -> anyhow::Result<()> {
    let Some(path) = config_path() else {
        anyhow::bail!("无法解析本机配置目录");
    };
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let c = normalize(config.clone());
    let s = serde_json::to_string_pretty(&c)?;
    std::fs::write(&path, s)?;
    Ok(())
}
