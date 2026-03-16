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

use crate::commands::{CookieStore, ProxySettings};

const LOCAL_SERVER_PORT: u16 = 19315;

#[derive(Debug, Deserialize)]
struct ProxyRequest {
    target: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
}

pub async fn start_local_server(
    _cookie_store: CookieStore,
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
        .and_then(|tail: warp::path::Tail| async move {
            let path = tail.as_str();
            handle_static_file(path).await
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

    // 创建 client：禁用自动 gzip 解压，手动处理压缩
    // 这样可以正确控制 content-length 和响应体
    let client = reqwest::Client::builder()
        .no_gzip()
        .no_deflate()
        .no_brotli()
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
    
    if let Some(ref custom_headers) = req.headers {
        // 先添加 PWA 提供的 headers（优先级高）
        for (key, value) in custom_headers {
            let key_lower = key.to_lowercase();
            // Range 头特殊处理，支持音频/视频流
            if key_lower == "range" {
                log::info!("[LocalServer] Adding Range header for streaming: {} = {}", key, value);
            }
            request_builder = request_builder.header(key, value);
            added_headers.insert(key_lower);
        }
    }

    // 添加 body
    if let Some(body) = req.body {
        request_builder = request_builder.body(body);
    }

    // 发送请求
    match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            
            // 如果 content-type 是通用的 octet-stream，尝试从 URL 推断
            if content_type == "application/octet-stream" {
                let target_lower = req.target.to_lowercase();
                let inferred = if target_lower.ends_with(".jpg") || target_lower.ends_with(".jpeg") {
                    "image/jpeg"
                } else if target_lower.ends_with(".png") {
                    "image/png"
                } else if target_lower.ends_with(".gif") {
                    "image/gif"
                } else if target_lower.ends_with(".webp") {
                    "image/webp"
                } else if target_lower.ends_with(".mp4") {
                    "video/mp4"
                } else if target_lower.ends_with(".mp3") {
                    "audio/mpeg"
                } else {
                    &content_type
                };
                if inferred != &content_type {
                    log::info!("[LocalServer] Inferred content-type from URL: {} -> {}", content_type, inferred);
                    content_type = inferred.to_string();
                }
            }
            
            // 对于 audio/mpeg，尝试使用 audio/mp3 以兼容 WebKitGTK
            // if content_type == "audio/mpeg" {
            //     content_type = "audio/mp3".to_string();
            //     log::info!("[LocalServer] Changed content-type from audio/mpeg to audio/mp3 for WebKitGTK compatibility");
            // }
            
            let content_length = response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok());
            
            log::info!("[LocalServer] Upstream response: status={}, content-type={}, content-length={:?}", 
                status, content_type, content_length);
            
            // 打印所有响应头用于调试
            log::info!("[LocalServer] All upstream headers:");
            for (key, value) in response.headers() {
                if let Ok(v) = value.to_str() {
                    log::info!("[LocalServer]   {}: {}", key, v);
                }
            }

            // 检查是否为流媒体（音频/视频）
            let is_streaming = content_type.starts_with("audio/") 
                || content_type.starts_with("video/")
                || response.status() == 206;  // 206 Partial Content

            // 复制响应头
            let mut response_builder = Response::builder().status(status);
            
            // 检查是否有压缩编码
            let has_encoding = response
                .headers()
                .get("content-encoding")
                .map(|v| !v.to_str().unwrap_or("").is_empty())
                .unwrap_or(false);

            // 复制需要的响应头（content-type 除外，使用我们修改后的）
            for (key, value) in response.headers() {
                let key_lower = key.as_str().to_lowercase();
                // 保留这些头对流媒体很重要（排除 content-type）
                // 如果有压缩编码，不复制 content-length（解压后会重新计算）
                if key_lower == "accept-ranges"
                    || key_lower == "content-range"
                    || key_lower == "etag"
                    || key_lower == "last-modified" {
                    if let Ok(v) = value.to_str() {
                        response_builder = response_builder.header(key.as_str(), v);
                    }
                }
                // 只有在没有压缩编码时才复制 content-length
                if key_lower == "content-length" && !has_encoding {
                    if let Ok(v) = value.to_str() {
                        response_builder = response_builder.header(key.as_str(), v);
                    }
                }
            }
            
            // 手动设置 content-type（使用可能修改后的值）
            response_builder = response_builder.header("Content-Type", &content_type);

            // 添加 CORS 头和安全头
            response_builder = response_builder
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
                .header("Access-Control-Allow-Headers", "*")
                .header("X-Content-Type-Options", "nosniff");  // 强制使用提供的 content-type，防止错误识别
            
            // 对于音频流，添加额外的兼容性头
            if is_streaming {
                response_builder = response_builder
                    .header("Accept-Ranges", "bytes");  // 明确支持 Range 请求
                
                // 如果是音频，尝试添加 Content-Disposition 帮助识别
                if content_type.starts_with("audio/") {
                    // 从 URL 中提取文件名
                    let filename = req.target
                        .split('/')
                        .last()
                        .and_then(|s| s.split('?').next())
                        .unwrap_or("audio.mp3");
                    response_builder = response_builder
                        .header("Content-Disposition", format!("inline; filename=\"{}\"", filename));
                }
            }

            // 根据请求类型处理响应
            if is_media {
                // 音视频：流式传输，无需处理 gzip（已禁用）
                log::info!("[LocalServer] Streaming media response");
                let stream = response.bytes_stream();
                let body = Body::wrap_stream(stream);
                Ok(response_builder.body(body).unwrap())
            } else {
                // 普通请求：检查并处理 gzip/deflate
                let encoding = response
                    .headers()
                    .get("content-encoding")
                    .map(|v| v.to_str().unwrap_or("").to_lowercase())
                    .unwrap_or_default();
                
                log::info!("[LocalServer] Response encoding: '{}'", encoding);

                let body_bytes = match response.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        log::error!("[LocalServer] Failed to read response body: {}", e);
                        return Ok(Response::builder()
                            .status(500)
                            .body(Body::from(format!("Error: {}", e)))
                            .unwrap());
                    }
                };
                
                // 解压 gzip 或 deflate
                let body = if encoding.contains("gzip") || body_bytes.starts_with(&[0x1f, 0x8b]) {
                    use std::io::Read;
                    let mut decoder = flate2::read::GzDecoder::new(&body_bytes[..]);
                    let mut decompressed = Vec::new();
                    match decoder.read_to_end(&mut decompressed) {
                        Ok(_) => decompressed,
                        Err(e) => {
                            log::error!("[LocalServer] Failed to decompress gzip: {}", e);
                            body_bytes.to_vec()
                        }
                    }
                } else if encoding.contains("deflate") {
                    use std::io::Read;
                    let mut decoder = flate2::read::ZlibDecoder::new(&body_bytes[..]);
                    let mut decompressed = Vec::new();
                    match decoder.read_to_end(&mut decompressed) {
                        Ok(_) => decompressed,
                        Err(e) => {
                            log::error!("[LocalServer] Failed to decompress deflate: {}", e);
                            body_bytes.to_vec()
                        }
                    }
                } else {
                    body_bytes.to_vec()
                };

                log::debug!("[LocalServer] Returning response, size: {} bytes", body.len());
                Ok(response_builder.body(Body::from(body)).unwrap())
            }
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

async fn handle_static_file(path: &str) -> Result<impl Reply, Infallible> {
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
    
    // 代理图片请求，禁用自动 gzip 解压
    let client = reqwest::Client::builder()
        .no_gzip()
        .no_deflate()
        .no_brotli()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    
    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str().ok().map(|s| (k.to_string(), s.to_string()))
                })
                .collect();
            
            // 检查是否有压缩编码
            let encoding = response
                .headers()
                .get("content-encoding")
                .map(|v| v.to_str().unwrap_or("").to_lowercase())
                .unwrap_or_default();

            let body_bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    log::error!("[LocalServer] Failed to read image: {}", e);
                    return Ok(Response::builder()
                        .status(500)
                        .body(Body::from(format!("Error: {}", e)))
                        .unwrap());
                }
            };

            // 解压 gzip 或 deflate
            let body_bytes = if encoding.contains("gzip") {
                use std::io::Read;
                let mut decoder = flate2::read::GzDecoder::new(&body_bytes[..]);
                let mut decompressed = Vec::new();
                match decoder.read_to_end(&mut decompressed) {
                    Ok(_) => bytes::Bytes::from(decompressed),
                    Err(e) => {
                        log::error!("[LocalServer] Failed to decompress gzip: {}", e);
                        body_bytes
                    }
                }
            } else if encoding.contains("deflate") {
                use std::io::Read;
                let mut decoder = flate2::read::ZlibDecoder::new(&body_bytes[..]);
                let mut decompressed = Vec::new();
                match decoder.read_to_end(&mut decompressed) {
                    Ok(_) => bytes::Bytes::from(decompressed),
                    Err(e) => {
                        log::error!("[LocalServer] Failed to decompress deflate: {}", e);
                        body_bytes
                    }
                }
            } else {
                body_bytes
            };
            
            // 从 URL 推断 MIME 类型
            let mime_type = if let Some(ct) = headers.get("content-type") {
                ct.clone()
            } else {
                // 从 URL 扩展名推断
                let ext = url.split('.').last().unwrap_or("").to_lowercase();
                match ext.as_str() {
                    "jpg" | "jpeg" => "image/jpeg",
                    "png" => "image/png",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    "bmp" => "image/bmp",
                    "svg" => "image/svg+xml",
                    _ => "application/octet-stream",
                }.to_string()
            };
            
            let mut response_builder = Response::builder().status(status);
            
            // 设置内容类型
            response_builder = response_builder.header("Content-Type", mime_type);
            
            // 设置内容长度
            if let Some(cl) = headers.get("content-length") {
                response_builder = response_builder.header("Content-Length", cl);
            } else {
                response_builder = response_builder.header("Content-Length", body_bytes.len().to_string());
            }
            
            // 添加 CORS 头
            response_builder = response_builder
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "public, max-age=3600");
            
            Ok(response_builder.body(Body::from(body_bytes)).unwrap())
        }
        Err(e) => {
            log::error!("[LocalServer] Image proxy failed: {}", e);
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
