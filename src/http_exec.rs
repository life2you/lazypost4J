//! 同步 HTTP 请求（在独立线程中调用，避免阻塞 TUI 主线程）。

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::Url;

/// 发送 HTTP 请求并返回展示用结果。
pub fn send_http(
    method: &str,
    full_url: &str,
    extra_headers: &[(String, String)],
    body: Option<&str>,
    timeout_secs: u64,
) -> Result<HttpResult, String> {
    let _ =
        Url::parse(full_url).map_err(|e| format!("URL 无效: {full_url} — {e}"))?;

    let m = method.trim().to_uppercase();
    if m == "ALL" {
        return Err("接口 method 为 ALL，请用 scan 查看或在源码中限定动词".into());
    }

    let method_inner = reqwest::Method::from_bytes(stripped_method_bytes(&m))
        .map_err(|_| format!("不支持的 HTTP 方法: {method}"))?;

    let mut headers = HeaderMap::new();
    for (k, v) in extra_headers {
        let name =
            HeaderName::from_bytes(k.as_bytes()).map_err(|e| format!("非法 Header 名: {e}"))?;
        let val =
            HeaderValue::from_str(v).map_err(|e| format!("非法 Header 值 ({k}): {e}"))?;
        headers.insert(name, val);
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs.clamp(1, 300)))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.request(method_inner, full_url).headers(headers);

    let send_body = body
        .map(|b| !b.trim().is_empty())
        .unwrap_or(false)
        && matches!(m.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");

    if send_body {
        let body_s = body.unwrap_or("{}").to_string();
        let has_ct = extra_headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
        req = req.body(body_s);
        if !has_ct {
            req = req.header(CONTENT_TYPE, "application/json");
        }
    }

    let start = Instant::now();
    let resp = req.send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let header_lines: Vec<String> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            format!(
                "{}: {}",
                k,
                v.to_str().unwrap_or("<binary>")
            )
        })
        .collect();
    let body_bytes = resp.bytes().map_err(|e| e.to_string())?;
    let body_text = String::from_utf8_lossy(&body_bytes).into_owned();

    Ok(HttpResult {
        status,
        elapsed_ms: start.elapsed().as_millis(),
        headers_text: header_lines.join("\n"),
        body: body_text,
    })
}

fn stripped_method_bytes(m: &str) -> &[u8] {
    m.trim().as_bytes()
}

#[derive(Debug, Clone)]
pub struct HttpResult {
    pub status: u16,
    pub elapsed_ms: u128,
    pub headers_text: String,
    pub body: String,
}

/// 合并 base、path（替换 `{key}`），并附加 query。
pub fn compose_url(
    base: &str,
    path_with_braces: &str,
    path_values: &std::collections::HashMap<String, String>,
    query_values: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let mut path = path_with_braces.to_string();
    for (k, v) in path_values {
        path = path.replace(&format!("{{{k}}}"), v);
    }

    let base = base.trim_end_matches('/');
    let path_part = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };

    let mut u = Url::parse(&format!("{base}{path_part}"))
        .map_err(|e| format!("URL 解析失败: {base}{path_part} — {e}"))?;

    {
        let mut q = u.query_pairs_mut();
        let mut keys: Vec<_> = query_values
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect();
        keys.sort();
        for k in keys {
            if let Some(v) = query_values.get(&k) {
                q.append_pair(&k, v);
            }
        }
    }

    Ok(u.to_string())
}
