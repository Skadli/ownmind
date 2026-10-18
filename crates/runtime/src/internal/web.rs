//! 联网工具：web_search（Brave / Tavily / SearXNG）与 web_fetch（HTML 转正文，防 SSRF）。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use ownmind_settings::api::{ResolvedSearch, SearchProvider, Settings};
use reqwest::Url;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Tool, ToolContext, ToolFuture, ToolOutput, ToolSpec, WebOptions};

const FETCH_MAX_BYTES: usize = 2 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 5;
const MAX_TEXT_CHARS: usize = 30_000;
const SEARCH_LIMIT: usize = 8;

pub(crate) fn web_tools(settings: Settings, options: WebOptions) -> Vec<Arc<dyn Tool>> {
    let shared = Arc::new(Shared { settings, options });
    vec![
        Arc::new(WebTool {
            shared: shared.clone(),
            fetch: false,
        }),
        Arc::new(WebTool {
            shared,
            fetch: true,
        }),
    ]
}

struct Shared {
    settings: Settings,
    options: WebOptions,
}

struct WebTool {
    shared: Arc<Shared>,
    fetch: bool,
}

#[derive(Deserialize)]
struct FetchIn {
    url: String,
}

#[derive(Deserialize)]
struct SearchIn {
    query: String,
}

impl Tool for WebTool {
    fn spec(&self) -> ToolSpec {
        if self.fetch {
            ToolSpec {
                name: "web_fetch".into(),
                description:
                    "抓取一个网页并转成正文文本（只读；拒绝内网地址；最大 2 MB、15 秒超时）。"
                        .into(),
                parameters: json!({ "type": "object", "properties": {
                    "url": { "type": "string", "description": "http(s) 网址" }
                }, "required": ["url"] }),
            }
        } else {
            ToolSpec {
                name: "web_search".into(),
                description: "联网搜索，返回标题、链接和摘要。".into(),
                parameters: json!({ "type": "object", "properties": {
                    "query": { "type": "string", "description": "搜索词" }
                }, "required": ["query"] }),
            }
        }
    }

    fn call(&self, _ctx: ToolContext, arguments: Value) -> ToolFuture {
        let shared = self.shared.clone();
        let fetch = self.fetch;
        Box::pin(async move {
            let r = if fetch {
                match serde_json::from_value::<FetchIn>(arguments) {
                    Ok(a) => fetch_page(&shared.options, &a.url).await,
                    Err(e) => Err(format!("参数不合法：{e}")),
                }
            } else {
                match serde_json::from_value::<SearchIn>(arguments) {
                    Ok(a) => search(&shared, &a.query).await,
                    Err(e) => Err(format!("参数不合法：{e}")),
                }
            };
            match r {
                Ok(t) => ToolOutput::ok(t),
                Err(m) => ToolOutput::error(m),
            }
        })
    }
}

// ---------- 搜索 ----------

struct Hit {
    title: String,
    url: String,
    snippet: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent("ownmind/0.1")
        .build()
        .map_err(|e| e.to_string())
}

async fn search(shared: &Shared, query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("搜索词不能为空".into());
    }
    let cfg = shared
        .settings
        .resolve_search()
        .await
        .map_err(|e| e.to_string())?
        .ok_or(
            "还没有配置联网搜索：请主人在 设置 → 账户 里填 Brave、Tavily 的 Key 或 SearXNG 地址",
        )?;
    let hits = run_search(&shared.options, &cfg, query).await?;
    if hits.is_empty() {
        return Ok("没有搜到结果".into());
    }
    Ok(hits
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet))
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn run_search(o: &WebOptions, cfg: &ResolvedSearch, q: &str) -> Result<Vec<Hit>, String> {
    let c = client()?;
    let key = cfg.api_key.clone().unwrap_or_default();
    let limit = SEARCH_LIMIT.to_string();
    let (req, list_path, snippet_key) = match cfg.provider {
        SearchProvider::Brave => {
            let url = o
                .brave_endpoint
                .clone()
                .unwrap_or_else(|| "https://api.search.brave.com/res/v1/web/search".into());
            (
                c.get(url)
                    .query(&[("q", q), ("count", limit.as_str())])
                    .header("X-Subscription-Token", key)
                    .header("Accept", "application/json"),
                "/web/results",
                "description",
            )
        }
        SearchProvider::Tavily => {
            let url = o
                .tavily_endpoint
                .clone()
                .unwrap_or_else(|| "https://api.tavily.com/search".into());
            (
                c.post(url)
                    .bearer_auth(key)
                    .json(&json!({ "query": q, "max_results": SEARCH_LIMIT })),
                "/results",
                "content",
            )
        }
        SearchProvider::Searxng => {
            let base = cfg.base_url.clone().ok_or("SearXNG 地址未配置")?;
            (
                c.get(format!("{base}/search"))
                    .query(&[("q", q), ("format", "json")]),
                "/results",
                "content",
            )
        }
    };
    let resp = req.send().await.map_err(|e| format!("搜索请求失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("搜索服务返回 {status}"));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("搜索结果解析失败：{e}"))?;
    let text = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    Ok(v.pointer(list_path)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(SEARCH_LIMIT)
                .map(|x| Hit {
                    title: html_to_text(&text(x, "title")),
                    url: text(x, "url"),
                    snippet: html_to_text(&text(x, snippet_key)),
                })
                .collect()
        })
        .unwrap_or_default())
}

// ---------- 抓取 ----------

/// 内网、回环、链路本地、保留地址一律拒绝。
pub(crate) fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => private_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return private_v4(v4);
            }
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 唯一本地
                || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 链路本地
                || v6 == Ipv6Addr::LOCALHOST
        }
    }
}

fn private_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (64..128).contains(&o[1])) // 100.64/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || o[0] >= 240
}

async fn fetch_page(o: &WebOptions, raw: &str) -> Result<String, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "网址不合法".to_owned())?;
    for _ in 0..=MAX_REDIRECTS {
        if !matches!(url.scheme(), "http" | "https") {
            return Err("只支持 http / https 网址".into());
        }
        let host = url
            .host_str()
            .ok_or("网址缺少主机名")?
            .trim_matches(['[', ']'])
            .to_owned();
        let port = url.port_or_known_default().ok_or("网址端口不合法")?;
        // 先解析再校验，并把连接钉在已校验的地址上，防止 DNS 重绑定。
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| format!("域名解析失败：{e}"))?
            .collect();
        if addrs.is_empty() {
            return Err("域名解析失败".into());
        }
        if !o.allow_private && addrs.iter().any(|a| is_private(a.ip())) {
            return Err("拒绝访问内网或本机地址".into());
        }
        let c = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &addrs)
            .user_agent("Mozilla/5.0 (compatible; ownmind/0.1)")
            .build()
            .map_err(|e| e.to_string())?;
        let mut resp = c
            .get(url.clone())
            .send()
            .await
            .map_err(|e| format!("抓取失败：{e}"))?;
        if resp.status().is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("重定向缺少 Location")?;
            url = url.join(loc).map_err(|_| "重定向地址不合法".to_owned())?;
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("网页返回 {}", resp.status()));
        }
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !(ctype.is_empty()
            || ctype.starts_with("text/")
            || ctype.contains("json")
            || ctype.contains("xml"))
        {
            return Err(format!("不支持的内容类型：{ctype}"));
        }
        let mut body = Vec::new();
        let mut cut = false;
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("读取失败：{e}"))? {
            if body.len() + chunk.len() > FETCH_MAX_BYTES {
                body.extend_from_slice(&chunk[..FETCH_MAX_BYTES - body.len()]);
                cut = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        let raw_text = String::from_utf8_lossy(&body).into_owned();
        let mut text = if ctype.contains("html") || ctype.is_empty() {
            html_to_text(&raw_text)
        } else {
            raw_text
        };
        if text.chars().count() > MAX_TEXT_CHARS {
            text = text.chars().take(MAX_TEXT_CHARS).collect();
            cut = true;
        }
        if cut {
            text.push_str("\n（内容过长，已截断）");
        }
        return Ok(format!("{url}\n\n{text}"));
    }
    Err("重定向次数过多".into())
}

/// 粗粒度 HTML 转正文：去 script/style/注释，标签换成空白，解码常见实体。
pub(crate) fn html_to_text(html: &str) -> String {
    const SKIPPED: [&str; 4] = ["script", "style", "noscript", "template"];
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |p| p + 3);
        } else if rest.starts_with('<') {
            let end = rest.find('>').map_or(rest.len(), |p| p + 1);
            let name: String = lower[i + 1..i + end]
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            i += end;
            if SKIPPED.contains(&name.as_str()) {
                let close = format!("</{name}");
                i = lower[i..].find(&close).map_or(html.len(), |p| {
                    let from = i + p;
                    from + lower[from..]
                        .find('>')
                        .map_or(lower.len() - from, |q| q + 1)
                });
            }
            out.push(' ');
        } else {
            let next = rest.find('<').unwrap_or(rest.len());
            out.push_str(&rest[..next]);
            i += next;
        }
    }
    let decoded = decode_entities(&out);
    let mut text = String::new();
    for line in decoded.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !l.is_empty() {
            text.push_str(&l);
            text.push('\n');
        }
    }
    text.trim_end().to_owned()
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let decoded = rest.find(';').filter(|e| *e <= 10).and_then(|e| {
            let name = &rest[1..e];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, n)) => {
                out.push(c);
                rest = &rest[n..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_text() {
        let h = "<html><head><style>p{}</style><script>var a='<b>';</script></head>\
                 <body><h1>标题</h1><!-- x --><p>a &amp; b&nbsp;&#65;</p></body></html>";
        assert_eq!(html_to_text(h), "标题 a & b A");
    }

    #[test]
    fn private_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(is_private(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(!is_private(ip.parse().unwrap()), "{ip}");
        }
    }
}
