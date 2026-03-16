use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use warp::{Filter, Reply};
use warp::http::Response;
use warp::hyper::Body;
use serde::Deserialize;

use crate::commands::ProxySettings;

const LOCAL_SERVER_PORT: u16 = 19315;

#[derive(Debug, Deserialize)]
struct ProxyRequest {
    target: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
}

pub async fn start_local_server(
    _proxy_settings: Arc<RwLock<Option<ProxySettings>>>,
) {
    // API 代理路由 - 普通请求，启用 gzip
    // POST 方式供前端 JS 使用
    let proxy_route_post = warp::path!("api" / "proxy")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::filters::header::headers_cloned())
        .and_then(|req: ProxyRequest, headers: warp::http::HeaderMap| async move {
            handle_proxy_request(req, headers, false).await
        });
    
    // GET 方式供 <img> 等标签直接使用
    let proxy_route_get = warp::path!("api" / "proxy")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::filters::header::headers_cloned())
        .and_then(|query: HashMap<String, String>, mut headers: warp::http::HeaderMap| async move {
            let mut target = query.get("url").cloned().unwrap_or_default();
            if target.is_empty() {
                let response: Response<Body> = Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Body::from("Missing 'url' parameter"))
                    .unwrap();
                return Ok::<Box<dyn Reply>, Infallible>(Box::new(response));
            }
            
            // 从查询参数中提取 header_ 开头的自定义 headers
            let mut custom_headers = HashMap::new();
            for (key, value) in &query {
                if key.starts_with("header_") {
                    let header_name = key.trim_start_matches("header_");
                    custom_headers.insert(header_name.to_string(), value.clone());
                }
            }
            
            // 自动设置 Referer（从目标 URL 提取域名）
            if !custom_headers.contains_key("Referer") && !custom_headers.contains_key("referer") {
                if !headers.contains_key("referer") && !headers.contains_key("Referer") {
                    if let Ok(url) = url::Url::parse(&target) {
                        if url.scheme() != "file" {
                            if let Some(host) = url.host_str() {
                                let referer = format!("{}://{}", url.scheme(), host);
                                log::info!("[LocalServer] Auto-set Referer: {}", referer);
                                custom_headers.insert("Referer".to_string(), referer);
                            }
                        }
                    }
                }
            }
            
            let req = ProxyRequest {
                target,
                method: Some("GET".to_string()),
                headers: if custom_headers.is_empty() { None } else { Some(custom_headers) },
                body: None,
            };
            let result = handle_proxy_request(req, headers, false).await;
            Ok::<Box<dyn Reply>, Infallible>(Box::new(result))
        });

    // 媒体代理路由 - 支持 GET (URL参数) 和 POST (JSON body)
    // GET 方式供 <video> <audio> 标签直接使用
    let media_route_get = warp::path!("media" / "proxy")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::filters::header::headers_cloned())
        .and_then(|query: HashMap<String, String>, mut headers: warp::http::HeaderMap| async move {
            let mut target = query.get("url").cloned().unwrap_or_default();
            if target.is_empty() {
                let response: Response<Body> = Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Body::from("Missing 'url' parameter"))
                    .unwrap();
                return Ok::<Box<dyn Reply>, Infallible>(Box::new(response));
            }
            
            // 处理本地文件路径
            // 支持格式: /path/to/file.mp4, file:///path/to/file.mp4, /computer/Music/file.mp3
            if target.starts_with('/') && !target.starts_with("//") {
                // 转换为 file:// URL
                target = format!("file://{}", target);
                log::info!("[LocalServer] Converted local path to URL: {}", target);
            }
            
            // 从查询参数中提取 header_ 开头的自定义 headers
            let mut custom_headers = HashMap::new();
            for (key, value) in &query {
                if key.starts_with("header_") {
                    let header_name = key.trim_start_matches("header_");
                    custom_headers.insert(header_name.to_string(), value.clone());
                }
            }
            
            // 自动设置 Referer（从目标 URL 提取域名，本地文件除外）
            if !custom_headers.contains_key("Referer") && !custom_headers.contains_key("referer") {
                if !headers.contains_key("referer") && !headers.contains_key("Referer") {
                    if let Ok(url) = url::Url::parse(&target) {
                        if url.scheme() != "file" {
                            if let Some(host) = url.host_str() {
                                let referer = format!("{}://{}", url.scheme(), host);
                                log::info!("[LocalServer] Auto-set Referer: {}", referer);
                                custom_headers.insert("Referer".to_string(), referer);
                            }
                        }
                    }
                }
            }
            
            let req = ProxyRequest {
                target,
                method: Some("GET".to_string()),
                headers: if custom_headers.is_empty() { None } else { Some(custom_headers) },
                body: None,
            };
            let result = handle_proxy_request(req, headers, true).await;
            Ok::<Box<dyn Reply>, Infallible>(Box::new(result))
        });
    
    // POST 方式供前端 JS 使用
    let media_route_post = warp::path!("media" / "proxy")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::filters::header::headers_cloned())
        .and_then(|req: ProxyRequest, headers: warp::http::HeaderMap| async move {
            handle_proxy_request(req, headers, true).await
        });

    // OPTIONS 预检路由 - 匹配所有路径
    let options_route = warp::options()
        .and(warp::path::full())
        .map(|_path: warp::path::FullPath| {
            Response::builder()
                .status(200)
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
                .header("Access-Control-Allow-Headers", "content-type, range, referer, accept, authorization, x-requested-with")
                .header("Access-Control-Max-Age", "86400")
                .body(Body::empty())
                .unwrap()
        });

    // 静态文件路由 (远程 URL 代理)
    let static_route = warp::path("static")
        .and(warp::path::tail())
        .and(warp::filters::header::headers_cloned())
        .and_then(|tail: warp::path::Tail, headers: warp::http::HeaderMap| async move {
            let path = tail.as_str();
            handle_static_file(path, headers).await
        });

    // 本地文件服务路由 /local/file/<encoded_path>
    let local_file_route = warp::path!("local" / "file" / ..)
        .and(warp::path::tail())
        .and(warp::filters::header::headers_cloned())
        .and_then(|tail: warp::path::Tail, headers: warp::http::HeaderMap| async move {
            let path = tail.as_str();
            handle_local_file(path, headers).await
        });

    // 组合所有路由（CORS 已手动添加在各响应中）
    let routes = options_route
        .or(proxy_route_get)
        .or(proxy_route_post)
        .or(media_route_get)
        .or(media_route_post)
        .or(static_route)
        .or(local_file_route);

    log::info!("[LocalServer] Starting on port {}", LOCAL_SERVER_PORT);
    
    // 使用 bind_ephemeral 获取绑定好的服务器
    let (addr, server) = warp::serve(routes)
        .bind_ephemeral(([127, 0, 0, 1], LOCAL_SERVER_PORT));
    
    log::info!("[LocalServer] Bound to {}", addr);
    
    // 在后台运行服务器
    tauri::async_runtime::spawn(server);
    
    // 等待一小段时间确保服务器就绪
    sleep(Duration::from_millis(100)).await;
    
    log::info!("[LocalServer] Ready on http://localhost:{}", LOCAL_SERVER_PORT);
}

async fn handle_proxy_request(
    req: ProxyRequest,
    http_headers: warp::http::HeaderMap,
    is_media: bool,
) -> Result<impl Reply, Infallible> {
    log::info!("[LocalServer] Received {} request: target={}, method={:?}", 
        if is_media { "media" } else { "proxy" }, req.target, req.method);

    // 创建 client：自动处理 gzip/deflate/brotli 压缩
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let method = req.method.unwrap_or_else(|| "GET".to_string());
    let mut request_builder = client.request(
        method.parse().unwrap_or(reqwest::Method::GET),
        &req.target
    );

    // 从 HTTP 请求中复制 headers（只复制安全的 headers，Referer/User-Agent 等敏感头由 PWA 提供）
    for (key, value) in &http_headers {
        let key_str = key.as_str().to_lowercase();
        // 只复制非敏感的、通用的 headers
        if key_str == "accept" 
            || key_str == "accept-language"
            || key_str == "accept-encoding"
            || key_str == "cache-control" 
            || key_str == "range" {  // 支持 Range 请求（音频/视频需要）
            if let Ok(value_str) = value.to_str() {
                // 如果 PWA 没有提供这个 header，才使用浏览器的
                if req.headers.as_ref().map_or(true, |h| !h.contains_key(key.as_str())) {
                    request_builder = request_builder.header(key.as_str(), value_str);
                    log::debug!("[LocalServer] Adding header from HTTP: {} = {}", key_str, value_str);
                }
            }
        }
    }

    // 从 body 的 headers 字段添加自定义 headers（Referer, User-Agent 等）
    // 这些由 PWA 完全控制
    // 使用 lowercase key 去重，PWA 提供的优先级更高
    let mut added_headers: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    // 不应该转发的 headers（hop-by-hop 或浏览器自动添加的）
    let hop_by_hop_headers: std::collections::HashSet<&str> = [
        "host", "connection", "keep-alive", "proxy-authenticate", 
        "proxy-authorization", "te", "trailers", "transfer-encoding", "upgrade",
        "sec-fetch-dest", "sec-fetch-mode", "sec-fetch-site", "sec-ch-ua", 
        "sec-ch-ua-mobile", "sec-ch-ua-platform", "x-request-key",
    ].iter().cloned().collect();
    
    if let Some(ref custom_headers) = req.headers {
        // 先添加 PWA 提供的 headers（优先级高）
        for (key, value) in custom_headers {
            let key_lower = key.to_lowercase();

            // 跳过 hop-by-hop headers
            if hop_by_hop_headers.contains(key_lower.as_str()) {
                log::debug!("[LocalServer] Skipping hop-by-hop header: {}", key);
                continue;
            }

            // Range 头特殊处理，支持音频/视频流
            if key_lower == "range" {
                log::info!("[LocalServer] Adding Range header for streaming: {} = {}", key, value);
            }
            request_builder = request_builder.header(key, value);
            added_headers.insert(key_lower);
        }

        // 调试日志：输出所有已添加的 PWA headers
        log::info!("[LocalServer] Added PWA headers: {:?}", custom_headers);
    }
    
    // 强制使用数据库中的 User-Agent（覆盖 PWA 提供的）
    if let Some(db_mutex) = crate::DB_CONN.get() {
        if let Ok(conn) = db_mutex.lock() {
            if let Ok(user_agent) = crate::db::get_user_agent(&conn) {
                if !user_agent.is_empty() {
                    log::info!("[LocalServer] Forcing User-Agent from DB: {}", &user_agent);
                    request_builder = request_builder.header("User-Agent", user_agent);
                }
            }
        }
    }

    // 添加 body
    if let Some(mut body) = req.body {
        // 检查 Content-Type，如果是 form-urlencoded 且 body 被 JSON 编码（带引号），解码它
        // 使用不区分大小写的方式查找 content-type header
        let content_type = req.headers.as_ref()
            .and_then(|h| {
                h.iter()
                    .find(|(k, _)| k.to_lowercase() == "content-type")
                    .map(|(_, v)| v.to_lowercase())
            });
        
        log::info!("[LocalServer] Request content-type: {}", content_type.as_ref().map_or("unknown", |v| v));
        log::info!("[LocalServer] Request body: {}", body);

        if content_type.as_ref().map_or(false, |ct| ct.contains("application/x-www-form-urlencoded")) {
            // 去掉可能的 JSON 字符串引号
            if body.starts_with('"') && body.ends_with('"') && body.len() >= 2 {
                body = body[1..body.len()-1].to_string();
                // 反转义
                body = body.replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\\"", "\"");
                log::info!("[LocalServer] Decoded form-urlencoded body: {}", body);
            }
        }
        request_builder = request_builder.body(body);
    }

    // 自动添加 Cookies（直接查数据库）
    if let Some(db_mutex) = crate::DB_CONN.get() {
        if let Ok(conn) = db_mutex.lock() {
            if let Ok(url) = url::Url::parse(&req.target) {
                if let Some(domain) = url.host_str() {
                    // 尝试 "browser" 和 "webview" 两种 app_id
                    let mut all_cookies = Vec::new();
                    
                    if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "browser", domain) {
                        for (k, v) in cookies {
                            all_cookies.push(format!("{}={}", k, v));
                        }
                    }
                    
                    if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "webview", domain) {
                        for (k, v) in cookies {
                            all_cookies.push(format!("{}={}", k, v));
                        }
                    }
                    
                    if !all_cookies.is_empty() {
                        let cookie_str = all_cookies.join("; ");
                        log::info!("[LocalServer] Adding cookies for {}: {}", domain, cookie_str);
                        request_builder = request_builder.header("Cookie", cookie_str);
                    }
                }
            }
        }
    }

    // 发送请求
    match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16();

            log::info!("[LocalServer] Upstream response: status={}", status);

            // 复制所有响应头（除了 hop-by-hop 头和 CORS 头）
            let mut response_builder = Response::builder().status(status);

            // hop-by-hop headers 不应该转发
            // CORS 头需要强制覆盖为 * 以支持跨域
            let hop_by_hop_response_headers: std::collections::HashSet<&str> = [
                "connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
                "te", "trailers", "transfer-encoding", "upgrade",
                "access-control-allow-origin", "access-control-allow-methods",
                "access-control-allow-headers", "access-control-max-age",
            ].iter().cloned().collect();

            // 保存 Set-Cookie 到数据库
            if let Ok(url) = url::Url::parse(&req.target) {
                if let Some(domain) = url.host_str() {
                    let mut new_cookies: Vec<(String, String)> = Vec::new();
                    for (hdr_key, hdr_value) in response.headers() {
                        if hdr_key.as_str().to_lowercase() == "set-cookie" {
                            if let Ok(cookie_str) = hdr_value.to_str() {
                                // 解析 Set-Cookie (格式: "name=value; ...")
                                if let Some(eq_pos) = cookie_str.find('=') {
                                    let cookie_name = cookie_str[..eq_pos].trim().to_string();
                                    let value_part = &cookie_str[eq_pos + 1..];
                                    // 取 value 部分（可能在 ; 之前）
                                    let cookie_value = value_part.split(';').next().unwrap_or("").trim().to_string();
                                    if !cookie_name.is_empty() {
                                        new_cookies.push((cookie_name, cookie_value));
                                    }
                                }
                            }
                        }
                    }
                    
                    if !new_cookies.is_empty() {
                        // 直接保存到数据库
                        if let Some(db_conn) = crate::DB_CONN.get() {
                            if let Ok(conn) = db_conn.lock() {
                                if let Err(e) = crate::db::save_cookies_batch(&conn, "browser", domain, &new_cookies) {
                                    log::error!("[LocalServer] Cookie save failed: {}", e);
                                } else {
                                    log::info!("[LocalServer] Saved {} cookies for {}", new_cookies.len(), domain);
                                }
                            }
                        }
                    }
                }
            }

            for (key, value) in response.headers() {
                let key_lower = key.as_str().to_lowercase();
                if !hop_by_hop_response_headers.contains(key_lower.as_str()) {
                    if let Ok(v) = value.to_str() {
                        response_builder = response_builder.header(key.as_str(), v);
                    }
                }
            }

            // 强制添加 CORS 头，确保跨域请求能正常工作
            response_builder = response_builder
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
                .header("Access-Control-Allow-Headers", "*")
                .header("X-Content-Type-Options", "nosniff");

            // 流式传输 body（reqwest 已经自动解压）
            let stream = response.bytes_stream();
            let body = Body::wrap_stream(stream);
            Ok(response_builder.body(body).unwrap())
        }
        Err(e) => {
            log::error!("[LocalServer] Proxy request failed: {}", e);
            Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(format!("{{\"error\": \"{}\"}}", e)))
                .unwrap())
        }
    }
}

async fn handle_static_file(path: &str, original_headers: warp::http::HeaderMap) -> Result<impl Reply, Infallible> {
    // URL 解码
    let url = match urlencoding::decode(path) {
        Ok(u) => u.to_string(),
        Err(_) => {
            return Ok(Response::builder()
                .status(400)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from("Invalid URL"))
                .unwrap());
        }
    };

    log::info!("[LocalServer] Static file proxy: {}", url);

    // 透传浏览器的原始 headers
    let mut headers = reqwest::header::HeaderMap::new();
    
    // 复制原始 headers（除了 host 和 connection）
    for (key, value) in &original_headers {
        let key_lower = key.as_str().to_lowercase();
        if key_lower != "host" && key_lower != "connection" && key_lower != "content-length" {
            if let Ok(name) = reqwest::header::HeaderName::from_bytes(key.as_str().as_bytes()) {
                if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                    headers.insert(name, val);
                }
            }
        }
    }
    
    // 解析 URL 获取域名
    let parsed_url = url::Url::parse(&url).ok();
    let domain_opt = parsed_url.as_ref().and_then(|u| u.host_str());
    
    // 从数据库覆盖 User-Agent 并添加 Cookies
    if let Some(db_mutex) = crate::DB_CONN.get() {
        if let Ok(conn) = db_mutex.lock() {
            // 覆盖 User-Agent
            if let Ok(user_agent) = crate::db::get_user_agent(&conn) {
                if !user_agent.is_empty() {
                    headers.insert(reqwest::header::USER_AGENT, user_agent.parse().unwrap());
                }
            }
            
            // 添加 Cookies
            if let Some(domain) = domain_opt {
                let mut all_cookies = Vec::new();
                
                if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "browser", domain) {
                    for (k, v) in cookies {
                        all_cookies.push(format!("{}={}", k, v));
                    }
                }
                
                if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "webview", domain) {
                    for (k, v) in cookies {
                        all_cookies.push(format!("{}={}", k, v));
                    }
                }
                
                if !all_cookies.is_empty() {
                    let cookie_str = all_cookies.join("; ");
                    log::info!("[LocalServer] Adding cookies for {}: {}", domain, cookie_str);
                    headers.insert(reqwest::header::COOKIE, cookie_str.parse().unwrap());
                }
            }
        }
    }

    // 自动处理压缩
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .default_headers(headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();

            // 复制响应头（除了 hop-by-hop、CORS 和 CORP 头）
            let mut response_builder = Response::builder().status(status);
            let hop_by_hop = ["connection", "keep-alive", "transfer-encoding", "upgrade",
                "access-control-allow-origin", "access-control-allow-methods",
                "access-control-allow-headers", "cross-origin-resource-policy"];

            for (key, value) in response.headers() {
                let key_lower = key.as_str().to_lowercase();
                if !hop_by_hop.contains(&key_lower.as_str()) {
                    if let Ok(v) = value.to_str() {
                        response_builder = response_builder.header(key.as_str(), v);
                    }
                }
            }

            // 强制添加 CORS 和 CORP 头，允许跨域访问
            response_builder = response_builder
                .header("Access-Control-Allow-Origin", "*")
                .header("Cross-Origin-Resource-Policy", "cross-origin")
                .header("Cache-Control", "public, max-age=3600");

            // 流式传输 body
            let stream = response.bytes_stream();
            Ok(response_builder.body(Body::wrap_stream(stream)).unwrap())
        }
        Err(e) => {
            log::error!("[LocalServer] Static file proxy failed: {}", e);
            Ok(Response::builder()
                .status(502)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(format!("Proxy error: {}", e)))
                .unwrap())
        }
    }
}

/// 处理本地文件请求，支持 Range 请求
async fn handle_local_file(
    path: &str,
    headers: warp::http::HeaderMap,
) -> Result<impl Reply, Infallible> {
    use std::io::{Read, Seek, SeekFrom};

    // URL 解码
    let decoded_path = match urlencoding::decode(path) {
        Ok(p) => p.to_string(),
        Err(_) => {
            return Ok(Response::builder()
                .status(400)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from("Invalid URL encoding"))
                .unwrap());
        }
    };

    log::info!("[LocalServer] Serving local file: {}", decoded_path);

    // 获取文件元数据
    let metadata = match std::fs::metadata(&decoded_path) {
        Ok(m) => m,
        Err(e) => {
            log::error!("[LocalServer] File not found: {}", e);
            return Ok(Response::builder()
                .status(404)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(format!("File not found: {}", e)))
                .unwrap());
        }
    };

    let file_size = metadata.len();

    // 获取 MIME 类型
    let ext = std::path::Path::new(&decoded_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wma" => "audio/x-ms-wma",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };

    // 解析 Range 请求头
    let range_header = headers.get("range").and_then(|v| v.to_str().ok());

    if let Some(range) = range_header {
        log::info!("[LocalServer] Range request: {}", range);

        if let Some(range_val) = range.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_val.split('-').collect();
            if parts.len() == 2 {
                let start: u64 = parts[0].parse().unwrap_or(0);
                let end: u64 = if parts[1].is_empty() {
                    file_size.saturating_sub(1)
                } else {
                    parts[1].parse().unwrap_or(file_size.saturating_sub(1))
                };
                let end = end.min(file_size.saturating_sub(1));

                if start <= end && start < file_size {
                    let length = end - start + 1;

                    match std::fs::File::open(&decoded_path) {
                        Ok(mut file) => {
                            if file.seek(SeekFrom::Start(start)).is_ok() {
                                let mut buffer = vec![0u8; length as usize];
                                if file.read_exact(&mut buffer).is_ok() {
                                    let response = Response::builder()
                                        .status(206)
                                        .header("Content-Type", mime_type)
                                        .header("Content-Length", length)
                                        .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
                                        .header("Accept-Ranges", "bytes")
                                        .header("Access-Control-Allow-Origin", "*")
                                        .header("Cross-Origin-Resource-Policy", "cross-origin")
                                        .body(Body::from(buffer))
                                        .unwrap();
                                    return Ok(response);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[LocalServer] Failed to open file: {}", e);
                        }
                    }
                }
            }
        }
    }

    // 无 Range 请求，返回整个文件
    match std::fs::read(&decoded_path) {
        Ok(data) => {
            let response = Response::builder()
                .status(200)
                .header("Content-Type", mime_type)
                .header("Content-Length", data.len())
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*")
                .header("Cross-Origin-Resource-Policy", "cross-origin")
                .body(Body::from(data))
                .unwrap();
            Ok(response)
        }
        Err(e) => {
            log::error!("[LocalServer] Failed to read file: {}", e);
            Ok(Response::builder()
                .status(500)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(format!("Read failed: {}", e)))
                .unwrap())
        }
    }
}

// 获取本地服务器端口
pub fn get_local_server_port() -> u16 {
    LOCAL_SERVER_PORT
}
