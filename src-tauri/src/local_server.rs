use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;
use url::Url;

/// 本地文件服务器状态
pub struct LocalFileServer {
    port: u16,
    actual_port: Mutex<u16>,
}

impl LocalFileServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            actual_port: Mutex::new(port),
        }
    }

    /// 生成直接访问本地文件的 URL (无状态模式，解决只能存一个文件的问题)
    pub fn get_proxy_url(&self, file_path: String) -> String {
        let port = *self.actual_port.lock().unwrap();
        let encoded_path = urlencoding::encode(&file_path);
        format!("http://localhost:{}/proxy?path={}", port, encoded_path)
    }

    /// 启动服务器
    pub fn start(&self) -> Result<u16, Box<dyn std::error::Error>> {
        let mut port = self.port;
        let listener = loop {
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
            match TcpListener::bind(addr) {
                Ok(l) => {
                    log::info!("[LocalServer] Listening on http://{}", addr);
                    break l;
                }
                Err(_) if port < 65535 => {
                    port += 1;
                }
                Err(e) => return Err(e.into()),
            }
        };

        *self.actual_port.lock().unwrap() = port;

        std::thread::spawn(move || {
            // 创建 Tokio runtime 用于异步处理
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        rt.spawn(async move {
                            if let Err(e) = handle_connection_async(stream).await {
                                log::debug!("[LocalServer] Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => log::error!("[LocalServer] Accept error: {}", e),
                }
            }
        });

        Ok(port)
    }
}

async fn handle_connection_async(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0u8; 4096];
    let n = stream.read(&mut buffer)?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let uri_str = parts[1];

    // 处理 CORS
    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 处理 /browser-tab.html
    if uri_str.starts_with("/browser-tab.html") {
        return handle_browser_tab_request(&mut stream);
    }

    // 处理 /adapt.min.js
    if uri_str.starts_with("/adapt.min.js") {
        return handle_adapt_js_request(&mut stream);
    }

    // 处理 /proxy 路由（本地文件代理）
    if uri_str.starts_with("/proxy") {
        return handle_proxy_request(&mut stream, method, uri_str);
    }

    // 处理 /web-proxy 路由（网站代理 - 用于 iframe 加载外部网站）
    if uri_str.starts_with("/web-proxy") {
        return handle_web_proxy_request(&mut stream, method, uri_str).await;
    }

    // 其他路由返回 404
    let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response.as_bytes())?;
    Ok(())
}

/// 提供 browser-tab.html
fn handle_browser_tab_request(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // 读取嵌入的 browser-tab.html 内容
    let html_content = include_str!("../../browser-tab.html");

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n{}",
        html_content.len(),
        html_content
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

/// 提供 adapt.min.js
fn handle_adapt_js_request(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let js_content = include_str!("../../adapt.min.js");

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: public, max-age=3600\r\nConnection: close\r\n\r\n{}",
        js_content.len(),
        js_content
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

/// 处理本地文件代理请求
fn handle_proxy_request(
    stream: &mut TcpStream,
    method: &str,
    uri_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 解析路径
    let file_path = if let Ok(url) = Url::parse(&format!("http://localhost{}", uri_str)) {
        url.query_pairs()
            .find(|(key, _)| key == "path")
            .map(|(_, val)| val.into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if file_path.is_empty() || !std::path::Path::new(&file_path).exists() {
        let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 获取文件信息
    let mut file = std::fs::File::open(&file_path)?;
    let file_size = file.metadata()?.len();

    // MIME
    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };

    // 解析 Range
    let mut range_start = 0u64;
    let mut range_end = file_size - 1;
    let mut is_partial = false;

    // 从请求头中解析 Range（简化处理，实际应该在 handle_connection 中解析）
    // 这里省略 Range 解析，直接返回整个文件

    // 处理 HEAD 请求
    if method == "HEAD" {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            mime_type, file_size
        );
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 流式传输文件
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        mime_type, file_size
    );
    stream.write_all(response.as_bytes())?;

    // 8KB 缓冲区流式读取
    let mut remaining = file_size;
    let mut chunk = [0u8; 8192];
    while remaining > 0 {
        let to_read = remaining.min(chunk.len() as u64) as usize;
        let n = file.read(&mut chunk[..to_read])?;
        if n == 0 {
            break;
        }
        if stream.write_all(&chunk[..n]).is_err() {
            log::debug!("[LocalServer] Client closed connection");
            break;
        }
        remaining -= n as u64;
    }

    Ok(())
}

static LOCAL_SERVER: once_cell::sync::OnceCell<LocalFileServer> = once_cell::sync::OnceCell::new();

pub fn init_local_server(port: u16) -> Result<u16, Box<dyn std::error::Error>> {
    let server = LocalFileServer::new(port);
    let actual_port = server.start()?;
    LOCAL_SERVER.set(server).ok();
    Ok(actual_port)
}

pub fn get_file_url(file_path: String) -> Option<String> {
    LOCAL_SERVER.get().map(|s| s.get_proxy_url(file_path))
}

pub fn get_server_port() -> Option<u16> {
    LOCAL_SERVER.get().map(|s| *s.actual_port.lock().unwrap())
}

use std::sync::Mutex as StdMutex;
use once_cell::sync::Lazy;

// 存储代理获取的 cookies: domain -> cookie string
static PROXY_COOKIES: Lazy<StdMutex<std::collections::HashMap<String, String>>> = 
    Lazy::new(|| StdMutex::new(std::collections::HashMap::new()));

/// 获取代理存储的 cookies
pub fn get_proxy_cookies(domain: &str) -> Option<String> {
    PROXY_COOKIES.lock().ok()?.get(domain).cloned()
}

/// 获取所有代理 cookies
pub fn get_all_proxy_cookies() -> std::collections::HashMap<String, String> {
    PROXY_COOKIES.lock().ok().map(|m| m.clone()).unwrap_or_default()
}

/// 处理网站代理请求（用于 iframe 加载外部网站）
async fn handle_web_proxy_request(
    stream: &mut TcpStream,
    method: &str,
    uri_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use url::Url;

    // 解析目标 URL
    let target_url = if let Ok(url) = Url::parse(&format!("http://localhost{}", uri_str)) {
        url.query_pairs()
            .find(|(key, _)| key == "url")
            .map(|(_, val)| val.into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if target_url.is_empty() {
        let response = "HTTP/1.1 400 Bad Request\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    log::info!("[WebProxy] Proxying: {}", target_url);

    // 创建 HTTP 客户端
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // 构建请求
    let mut request_builder = match method {
        "GET" => client.get(&target_url),
        "POST" => client.post(&target_url),
        "PUT" => client.put(&target_url),
        "DELETE" => client.delete(&target_url),
        "HEAD" => client.head(&target_url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, &target_url),
        _ => client.get(&target_url),
    };

    // 添加 headers（模拟浏览器）
    request_builder = request_builder
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Connection", "keep-alive");

    // 获取该域名的已有 cookies 并带上
    let domain = extract_domain_from_url(&target_url);
    if let Some(existing_cookies) = get_proxy_cookies(&domain) {
        request_builder = request_builder.header("Cookie", existing_cookies);
    }

    // 发送请求
    let response = match request_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            log::error!("[WebProxy] Request failed: {}", e);
            let response = format!(
                "HTTP/1.1 502 Bad Gateway\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                e.to_string().len(),
                e
            );
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }
    };

    let status = response.status();
    
    // 复制需要的 header 数据（避免借用问题）
    let headers_map: std::collections::HashMap<String, String> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_string())))
        .collect();

    // 拦截并存储 cookies
    if let Some(set_cookie) = response.headers().get_all("set-cookie").iter().next() {
        if let Ok(cookie_str) = set_cookie.to_str() {
            log::info!("[WebProxy] Captured cookie for {}: {}", domain, cookie_str);
            
            // 存储 cookie
            if let Ok(mut cookies) = PROXY_COOKIES.lock() {
                let entry = cookies.entry(domain.clone()).or_insert_with(String::new);
                if !entry.is_empty() {
                    entry.push_str("; ");
                }
                // 简化处理，只存储 key=value 部分
                let simple_cookie = cookie_str.split(';').next().unwrap_or(cookie_str);
                entry.push_str(simple_cookie);
            }
        }
    }

    // 读取响应体
    let body_bytes = response.bytes().await.unwrap_or_default();

    // 构建响应头（删除安全头）
    let mut response_headers = String::new();
    for (key, value) in &headers_map {
        let key_str = key.to_lowercase();
        // 跳过阻止 iframe 的安全头
        if key_str == "x-frame-options" 
            || key_str == "content-security-policy"
            || key_str == "content-security-policy-report-only" {
            continue;
        }
        response_headers.push_str(&format!("{}: {}\r\n", key, value));
    }

    // 添加 CORS 头
    response_headers.push_str("Access-Control-Allow-Origin: *\r\n");

    // 如果是 HTML，注入 adapt.js
    let content_type = headers_map
        .get("content-type")
        .unwrap_or(&"application/octet-stream".to_string())
        .clone();

    let final_body = if content_type.contains("text/html") {
        // 尝试注入 adapt.js
        let body_str = String::from_utf8_lossy(&body_bytes);
        let adapted_body = inject_adapt_script(&body_str, &target_url);
        adapted_body.into_bytes()
    } else {
        body_bytes.to_vec()
    };

    // 发送响应
    let response = format!(
        "HTTP/1.1 {} {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK"),
        response_headers,
        final_body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(&final_body)?;

    Ok(())
}

/// 从 URL 提取域名
fn extract_domain_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// 在 HTML 中注入 adapt.js
fn inject_adapt_script(html: &str, base_url: &str) -> String {
    // 找到 </head> 或 <body> 标签，在前面注入脚本
    let script = format!(
        r#"<script>
            window.__WEB_PROXY_BASE__ = "{}";
            window.__IS_WEB_PROXY__ = true;
        </script>
        <script src="http://localhost:{}/adapt.min.js"></script>"#,
        base_url,
        get_server_port().unwrap_or(8765)
    );

    if let Some(pos) = html.find("</head>") {
        let mut result = html[..pos].to_string();
        result.push_str(&script);
        result.push_str(&html[pos..]);
        result
    } else if let Some(pos) = html.find("<body") {
        let mut result = html[..pos].to_string();
        result.push_str(&script);
        result.push_str(&html[pos..]);
        result
    } else {
        // 直接在开头注入
        format!("{}\n{}", script, html)
    }
}
