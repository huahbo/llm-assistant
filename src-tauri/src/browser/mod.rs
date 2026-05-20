//! 无头浏览器服务模块
//!
//! 提供统一的 URL 内容抓取接口，优先使用 CDP 无头浏览器渲染（Chrome/Edge），
//! 失败后退回静态 HTTP 抓取。
//!
//! 本模块供两处复用：
//! 1. `ingest_service` — URL 摄入流程
//! 2. `agent_chat` — Agent 对话中的 fetch_url_context 命令

use headless_chrome::{Browser, LaunchOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── 公开数据结构 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub title: String,
    pub text: String,
    pub url: String,
    pub domain: String,
    pub char_count: usize,
    /// 抓取方式：`"cdp_chrome"` | `"cdp_edge"` | `"static_http"`
    pub fetch_method: String,
}

#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub timeout_secs: u64,
    pub virtual_time_budget_ms: u32,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            virtual_time_budget_ms: 5000,
        }
    }
}

// ─── 主入口 ───────────────────────────────────────────────────────────────────

/// 抓取指定 URL 的内容。
///
/// 优先通过 CDP 协议启动本机 Chrome/Edge 无头浏览器渲染（JS 完整执行），
/// 失败或内容过少时退回静态 HTTP 抓取。
pub async fn fetch_url(url: &str, options: FetchOptions) -> Result<FetchResult, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("URL 不能为空".to_string());
    }

    // 1. 尝试 CDP 无头浏览器（带 timeout）
    let timeout_dur = std::time::Duration::from_secs(options.timeout_secs);
    let url_clone = url.clone();
    let opts_clone = options.clone();
    let cdp_result = tokio::time::timeout(
        timeout_dur,
        tokio::task::spawn_blocking(move || {
            fetch_with_cdp_blocking(&url_clone, opts_clone)
        }),
    )
    .await;

    match cdp_result {
        Ok(Ok(Ok(result))) => return Ok(result),
        Ok(Ok(Err(_))) | Ok(Err(_)) | Err(_) => {
            // CDP 失败或超时，降级到静态 HTTP
        }
    }

    // 2. 静态 HTTP 兜底
    fetch_with_http(&url).await
}

// ─── CDP 抓取 ─────────────────────────────────────────────────────────────────

/// 在阻塞线程中通过 headless_chrome 启动无头浏览器，返回 FetchResult。
/// 供 spawn_blocking 调用。
fn fetch_with_cdp_blocking(url: &str, options: FetchOptions) -> Result<FetchResult, String> {
    let (exe_path, browser_name) = find_browser().ok_or_else(|| {
        "未找到 Chrome 或 Edge，无法启动无头浏览器".to_string()
    })?;

    let result = std::panic::catch_unwind(|| {
        do_cdp_fetch(url, &exe_path, browser_name, &options)
    });

    match result {
        Ok(inner) => inner,
        Err(_) => Err("CDP 无头浏览器内部 panic".to_string()),
    }
}

/// 实际的 CDP 抓取逻辑（被 catch_unwind 包裹）。
fn do_cdp_fetch(
    url: &str,
    exe_path: &PathBuf,
    browser_name: &'static str,
    options: &FetchOptions,
) -> Result<FetchResult, String> {
    let launch_opts = LaunchOptions {
        path: Some(exe_path.clone()),
        headless: true,
        sandbox: false,
        window_size: Some((1280, 800)),
        ..Default::default()
    };

    let browser =
        Browser::new(launch_opts).map_err(|e| format!("启动无头浏览器失败：{}", e))?;

    let tab = browser
        .new_tab()
        .map_err(|e| format!("打开新标签页失败：{}", e))?;

    // 设置虚拟时间预算（毫秒），让 SPA 有机会完成渲染
    let vt_ms = options.virtual_time_budget_ms;
    let _ = tab.evaluate(
        &format!(
            "chrome.runtime?.sendMessage({{ virtualTimeBudget: {} }})",
            vt_ms
        ),
        false,
    );

    tab.navigate_to(url)
        .map_err(|e| format!("导航到 URL 失败：{}", e))?
        .wait_until_navigated()
        .map_err(|e| format!("等待页面加载失败：{}", e))?;

    let content = tab
        .get_content()
        .map_err(|e| format!("获取页面内容失败：{}", e))?;

    let text = html_to_text(&content);
    if text.len() < 100 {
        return Err(format!(
            "CDP 渲染内容过少（{} 字符），视为失败",
            text.len()
        ));
    }

    let title = extract_title(&content);
    let domain = extract_domain(url);
    let char_count = text.len();
    let fetch_method = format!("cdp_{}", browser_name);

    Ok(FetchResult {
        title,
        text,
        url: url.to_string(),
        domain,
        char_count,
        fetch_method,
    })
}

// ─── 静态 HTTP 抓取 ───────────────────────────────────────────────────────────

/// 用 reqwest 静态抓取 URL，提取纯文本。
async fn fetch_with_http(url: &str) -> Result<FetchResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败：{}", e))?;

    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("HTTP 请求失败：{}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP 请求返回非成功状态码：{}", response.status()));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("读取响应体失败：{}", e))?;

    let text = html_to_text(&html);
    let title = extract_title(&html);
    let domain = extract_domain(url);
    let char_count = text.len();

    Ok(FetchResult {
        title,
        text,
        url: url.to_string(),
        domain,
        char_count,
        fetch_method: "static_http".to_string(),
    })
}

// ─── 浏览器查找 ───────────────────────────────────────────────────────────────

/// 查找 Chrome 可执行文件，返回路径和浏览器名称 `"chrome"`。
fn find_chrome_exe() -> Option<(PathBuf, &'static str)> {
    let candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];
    candidates
        .iter()
        .map(std::path::Path::new)
        .find(|p| p.exists())
        .map(|p| (p.to_path_buf(), "chrome"))
}

/// 查找 Edge 可执行文件，返回路径和浏览器名称 `"edge"`。
fn find_edge_exe() -> Option<(PathBuf, &'static str)> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    candidates
        .iter()
        .map(std::path::Path::new)
        .find(|p| p.exists())
        .map(|p| (p.to_path_buf(), "edge"))
}

/// Chrome 优先，Edge 次之。两者都没有则返回 None。
fn find_browser() -> Option<(PathBuf, &'static str)> {
    find_chrome_exe().or_else(find_edge_exe)
}

// ─── 工具函数 ─────────────────────────────────────────────────────────────────

/// 从 HTML 中提取 `<title>` 标签内容，清理 HTML 实体。
fn extract_title(html: &str) -> String {
    let title_re =
        regex::Regex::new(r"(?si)<title[^>]*>(.*?)</title\s*>").expect("invalid title regex");
    if let Some(caps) = title_re.captures(html) {
        let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        return raw
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ")
            .trim()
            .to_string();
    }
    String::new()
}

/// 从 URL 字符串解析域名部分（取 `://` 后第三段）。
fn extract_domain(url: &str) -> String {
    url.split('/')
        .nth(2)
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_string()
}

/// 将 HTML 转为纯文本：去除 script/style/head 块，去除剩余标签，合并空白。
///
/// 供 `ingest_service` 复用。
/// 注意：Rust regex 不支持反向引用（\1），每种块元素单独一个 pattern。
pub(crate) fn html_to_text(html: &str) -> String {
    // 各块元素独立 pattern，避免反向引用（Rust regex 不支持）
    let script_re = regex::Regex::new(r"(?si)<script[^>]*>.*?</script\s*>").expect("regex");
    let style_re = regex::Regex::new(r"(?si)<style[^>]*>.*?</style\s*>").expect("regex");
    let head_re = regex::Regex::new(r"(?si)<head[^>]*>.*?</head\s*>").expect("regex");
    let noscript_re = regex::Regex::new(r"(?si)<noscript[^>]*>.*?</noscript\s*>").expect("regex");
    let template_re = regex::Regex::new(r"(?si)<template[^>]*>.*?</template\s*>").expect("regex");
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("regex");
    let ws_re = regex::Regex::new(r"[ \t]{2,}").expect("regex");
    let nl_re = regex::Regex::new(r"\n{3,}").expect("regex");

    let text = script_re.replace_all(html, " ");
    let text = style_re.replace_all(&text, " ");
    let text = head_re.replace_all(&text, " ");
    let text = noscript_re.replace_all(&text, " ");
    let text = template_re.replace_all(&text, " ");
    let text = tag_re.replace_all(&text, " ");
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let text = ws_re.replace_all(&text, " ");
    let text = nl_re.replace_all(&text, "\n\n");
    text.trim().to_string()
}

// ─── 单元测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_strips_script_tags() {
        let html = "<html><head><title>T</title></head><body><script>var x=1;</script><p>Hello World</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Hello World"), "should contain body text");
        assert!(!text.contains("var x"), "should strip script content");
    }

    #[test]
    fn html_to_text_decodes_entities() {
        let html = "<p>&amp; &lt; &gt; &nbsp;</p>";
        let text = html_to_text(html);
        assert!(text.contains("&"), "should decode &amp;");
        assert!(text.contains("<"), "should decode &lt;");
    }

    #[test]
    fn extract_domain_works() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
        assert_eq!(
            extract_domain("https://sub.example.com/a/b"),
            "sub.example.com"
        );
    }
}
