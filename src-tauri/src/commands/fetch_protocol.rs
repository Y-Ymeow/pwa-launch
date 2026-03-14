#[derive(serde::Deserialize)]
struct ProxyRequest {
    target: String,
    method: Option<String>,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<serde_json::Value>,
}

/// 处理 fetch:// 协议请求（异步版本，避免阻塞主线程）
/// URL 格式:
/// - Linux: fetch://localhost/proxy
/// - Android: http://fetch.localhost/proxy
/// 从 POST body 中解析目标 URL 和请求信息
pub async fn handle_fetch_request_async(
    request: &tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let uri = request.uri().to_string();
    
    // 验证 URL 格式（fetch://localhost/ 或 http://fetch.localhost/）
    let valid_url = uri.starts_with("fetch://localhost/") || uri.starts_with("http://fetch.localhost/");
    if !valid_url {
        return Err(format!("Invalid fetch protocol URL: {}", uri));
    }
    
    // 解析 POST body 中的 JSON
    let body = request.body();
    let proxy_req: ProxyRequest = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(e) => {
            return Err(format!("Failed to parse proxy request body: {}", e));
        }
    };
    
    let target_url = proxy_req.target;
    let method = proxy_req.method.unwrap_or_else(|| "GET".to_string());
    
    log::info!("[FetchProtocol] {} {}", method, target_url);
    
    // 使用异步 reqwest 客户端
    let client = reqwest::Client::new();
    let http_method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };
    
    // 构建请求
    let mut request_builder = client.request(http_method, &target_url);
    
    // 复制从 body 解析的请求头
    if let Some(headers) = proxy_req.headers {
        for (key, value) in headers {
            let key_str = key.to_lowercase();
            if key_str != "host" && key_str != "origin" && key_str != "referer" && key_str != "content-length" {
                request_builder = request_builder.header(key, value);
            }
        }
    }
    
    // 添加常用头
    request_builder = request_builder
        .header("User-Agent", "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36")
        .header("Accept", "*/*")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8");
    
    // 设置请求 body（如果有）
    if let Some(body_value) = proxy_req.body {
        let body_str = match body_value {
            serde_json::Value::String(s) => s,
            _ => body_value.to_string(),
        };
        request_builder = request_builder.body(body_str);
    }

    // log builder
    log::info!("[FetchProtocol] Request: {:?}", request_builder);
    
    // 发送异步请求
    let response = match request_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            log::error!("[FetchProtocol] Request failed: {}", e);
            return Ok(tauri::http::Response::builder()
                .status(502)
                .header("Content-Type", "text/plain")
                .body(format!("Fetch error: {}", e).into_bytes())
                .unwrap());
        }
    };
    
    let status = response.status();
    
    // 检查是否是 gzip 压缩
    let is_gzip = response.headers()
        .get("content-encoding")
        .map(|v| v.to_str().unwrap_or("").contains("gzip"))
        .unwrap_or(false);
    
    // 构建响应
    let mut response_builder = tauri::http::Response::builder()
        .status(status.as_u16());
    
    // 复制响应头（跳过 content-encoding，因为我们要解压）
    for (key, value) in response.headers() {
        let key_str = key.as_str().to_lowercase();
        // 跳过压缩相关的头，因为内容已解压
        if key_str == "content-encoding" || key_str == "content-length" {
            continue;
        }
        if let Ok(val) = value.to_str() {
            response_builder = response_builder.header(key.as_str(), val);
        }
    }
    
    // 添加 CORS 头
    response_builder = response_builder
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
        .header("Access-Control-Allow-Headers", "*");
    
    // 异步读取响应体
    let body_bytes = match response.bytes().await {
        Ok(b) => b.to_vec(),
        Err(e) => {
            log::error!("[FetchProtocol] Failed to read body: {}", e);
            return Err(e.to_string());
        }
    };
    
    // 如果是 gzip 压缩，解压它
    let body = if is_gzip {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(&body_bytes[..]);
        let mut decompressed = Vec::new();
        match decoder.read_to_end(&mut decompressed) {
            Ok(_) => decompressed,
            Err(e) => {
                log::error!("[FetchProtocol] Failed to decompress gzip: {}", e);
                body_bytes // 解压失败则返回原始数据
            }
        }
    } else {
        body_bytes
    };
    
    Ok(response_builder.body(body).unwrap())
}

/// 同步包装（用于 Tauri 协议注册）
pub fn handle_fetch_request(
    request: &tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    // 使用 block_on 在异步运行时中执行，避免阻塞主线程
    tauri::async_runtime::block_on(handle_fetch_request_async(request))
}
