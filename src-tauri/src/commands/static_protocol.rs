use http::Response;
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

/// 处理 static 协议请求
pub fn handle_static_request(
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let path = request.uri().path();

    // URL 解码
    let decoded_path = match urlencoding::decode(path) {
        Ok(p) => p.to_string(),
        Err(_) => path.to_string(),
    };

    // 处理路径：去掉开头的 /
    let file_path = if decoded_path.starts_with('/') {
        decoded_path[1..].to_string()
    } else {
        decoded_path
    };

    log::info!("[static] 请求: {}", file_path);

    // 如果是远程 URL (http:// 或 https://)，代理请求
    if file_path.starts_with("http://") || file_path.starts_with("https://") {
        return handle_remote_request(&file_path, &request);
    }

    // 获取文件元数据
    let metadata = match std::fs::metadata(&file_path) {
        Ok(m) => m,
        Err(e) => {
            log::error!("[static] 文件不存在: {}", e);
            return Response::builder()
                .status(404)
                .body(format!("File not found: {}", e).into_bytes())
                .unwrap();
        }
    };

    let file_size = metadata.len();

    // 获取 MIME 类型
    let ext = std::path::Path::new(&file_path)
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
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };

    // 解析 Range 请求头
    let range_header = request
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok());

    if let Some(range) = range_header {
        log::info!("[static] Range 请求: {}", range);

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

                    match std::fs::File::open(&file_path) {
                        Ok(mut file) => {
                            if file.seek(SeekFrom::Start(start)).is_ok() {
                                let mut buffer = vec![0u8; length as usize];
                                if file.read_exact(&mut buffer).is_ok() {
                                    return Response::builder()
                                        .status(206)
                                        .header("Content-Type", mime_type)
                                        .header("Content-Length", length)
                                        .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
                                        .header("Accept-Ranges", "bytes")
                                        .header("Access-Control-Allow-Origin", "*")
                                        .body(buffer)
                                        .unwrap();
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[static] 打开文件失败: {}", e);
                        }
                    }
                }
            }
        }
    }

    // 无 Range 请求，返回整个文件
    match std::fs::read(&file_path) {
        Ok(data) => Response::builder()
            .header("Content-Type", mime_type)
            .header("Content-Length", data.len())
            .header("Accept-Ranges", "bytes")
            .header("Access-Control-Allow-Origin", "*")
            .body(data)
            .unwrap(),
        Err(e) => {
            log::error!("[static] 读取文件失败: {}", e);
            Response::builder()
                .status(500)
                .body(format!("Read failed: {}", e).into_bytes())
                .unwrap()
        }
    }
}

/// 处理远程 HTTP 请求代理（使用 reqwest blocking client，支持 SOCKS5）
fn handle_remote_request(
    url: &str,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    log::info!("[static] 代理远程请求: {}", url);

    // 创建 reqwest blocking client，支持 SOCKS5
    let mut client_builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))  // 30 秒超时
        .connect_timeout(Duration::from_secs(10));  // 10 秒连接超时

    // 检查是否有代理配置
    // 注意：这里简化处理，实际应该从全局配置读取
    // 由于 static_protocol 是同步上下文，无法直接访问 State
    // 这里使用环境变量作为临时方案
    if let Ok(proxy_url) = std::env::var("PWA_PROXY_URL") {
        log::info!("[static] 使用代理: {}", proxy_url);
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            client_builder = client_builder.proxy(proxy);
        }
    }

    let client = match client_builder.build() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[static] 创建 HTTP 客户端失败: {}", e);
            return Response::builder()
                .status(500)
                .body(format!("Client build failed: {}", e).into_bytes())
                .unwrap();
        }
    };

    // 构建请求
    let mut req_builder = client.get(url);

    // 添加 Range 头（如果有）
    if let Some(range) = request.headers().get("Range").and_then(|v| v.to_str().ok()) {
        req_builder = req_builder.header("Range", range);
    }

    // 添加 Referer
    if let Ok(url_obj) = url.parse::<url::Url>() {
        let referer = format!("{}://{}/", url_obj.scheme(), url_obj.host_str().unwrap_or(""));
        req_builder = req_builder.header("Referer", referer);
    }

    match req_builder.send() {
        Ok(response) => {
            let status = response.status().as_u16();
            log::info!("[static] 远程响应状态: {}", status);

            // 获取响应头
            let content_type = response
                .headers()
                .get("Content-Type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let content_range = response
                .headers()
                .get("Content-Range")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // 读取响应体（带大小限制，避免内存溢出）
            let body = match response.bytes() {
                Ok(bytes) => {
                    // 限制最大 50MB
                    if bytes.len() > 50 * 1024 * 1024 {
                        log::error!("[static] 响应体过大: {} bytes", bytes.len());
                        return Response::builder()
                            .status(413)
                            .body("Payload Too Large (max 50MB)".as_bytes().to_vec())
                            .unwrap();
                    }
                    bytes.to_vec()
                }
                Err(e) => {
                    log::error!("[static] 读取远程响应失败: {}", e);
                    return Response::builder()
                        .status(500)
                        .body(format!("Read remote failed: {}", e).into_bytes())
                        .unwrap();
                }
            };

            log::info!("[static] 远程响应大小: {} bytes", body.len());

            // 构建响应
            let mut builder = Response::builder()
                .status(status)
                .header("Content-Type", content_type)
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*");

            if let Some(range) = content_range {
                builder = builder.header("Content-Range", range);
            }

            builder.body(body).unwrap()
        }
        Err(e) => {
            log::error!("[static] 远程请求失败: {}", e);
            Response::builder()
                .status(502)
                .body(format!("Proxy failed: {}", e).into_bytes())
                .unwrap()
        }
    }
}
