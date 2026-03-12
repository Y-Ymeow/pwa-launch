use std::collections::HashMap;
use tauri::State;

use super::{extract_domain, CommandResponse, CookieStore, ProxyConfig};

/// 代理 fetch 请求 - 解决 CORS 问题
#[tauri::command]
pub async fn proxy_fetch(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    cookie_store: State<'_, CookieStore>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<serde_json::Value>, String> {
    // 清洗 body：去除 JSON 序列化带来的多余引号
    let body = body.map(|b| {
        let trimmed = b.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
            // 去掉外层引号并处理转义字符
            let inner = &trimmed[1..trimmed.len()-1];
            inner.replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\\"", "\"").replace("\\\\", "\\")
        } else {
            b
        }
    });
    
    log::info!("代理请求：{} {}", method, url);
    log::info!("Headers: {:?}", headers);

    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    
    // 优先使用 WebView 同步的 cookies（验证助手同步的）
    let webview_cookie_header = cookies
        .get("webview")
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| {
            c.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.is_empty());
    
    // 如果没有 WebView cookies，使用默认的
    let cookie_header = webview_cookie_header.or_else(|| {
        cookies
            .get("default")
            .and_then(|app_cookies| app_cookies.get(&domain))
            .map(|c| {
                c.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
    }).unwrap_or_default();
    
    if !cookie_header.is_empty() {
        log::info!("使用 Cookies: {}", cookie_header);
    }
    
    drop(cookies);

    let mut client_builder = reqwest::Client::builder().default_headers({
        let mut headers = reqwest::header::HeaderMap::new();
        if !cookie_header.is_empty() {
            headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
        }
        headers
    });

    log::info!("Body: {:?}", body);

    let proxy = proxy_config.read().await;
    if let Some(proxy_settings) = proxy.as_ref() {
        let proxy_url = if let (Some(user), Some(pass)) =
            (&proxy_settings.username, &proxy_settings.password)
        {
            format!(
                "{}://{}:{}@{}",
                proxy_settings.url.split("://").next().unwrap_or("http"),
                user,
                pass,
                proxy_settings
                    .url
                    .split("://")
                    .last()
                    .unwrap_or(&proxy_settings.url)
            )
        } else {
            proxy_settings.url.clone()
        };

        client_builder = client_builder
            .proxy(reqwest::Proxy::all(&proxy_url).map_err(|e| format!("代理配置失败：{}", e))?);
    }
    drop(proxy);

    let client = client_builder
        .build()
        .map_err(|e| format!("创建客户端失败：{}", e))?;

    // 处理 method 大小写不敏感
    let method_upper = method.to_uppercase();
    
    // 处理 GET 请求带 body 的情况 - 将 body 转为 query 参数
    let final_url = if method_upper == "GET" && body.is_some() {
        let body_str = body.as_ref().unwrap();
        // 尝试解析 body 为 form 参数并添加到 URL
        if body_str.starts_with('"') && body_str.ends_with('"') {
            // 去除外层引号
            let clean_body = &body_str[1..body_str.len()-1];
            if url.contains('?') {
                format!("{}&{}", url, clean_body)
            } else {
                format!("{}?{}", url, clean_body)
            }
        } else {
            url.clone()
        }
    } else {
        url.clone()
    };

    let mut req_builder = match method_upper.as_str() {
        "GET" => client.get(&final_url),
        "POST" => client.post(&final_url),
        "PUT" => client.put(&final_url),
        "DELETE" => client.delete(&final_url),
        "PATCH" => client.patch(&final_url),
        "HEAD" => client.head(&final_url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, &final_url),
        _ => client.get(&final_url),
    };

    // 添加用户自定义 headers（除了 cookie，cookie 单独处理）
    for (key, value) in headers {
        if key.to_lowercase() != "cookie" {
            req_builder = req_builder.header(&key, value);
        }
    }

    log::info!("Request: {:?}", req_builder);

    // GET 请求不发送 body（已经转为 query 参数）
    if method_upper != "GET" {
        if let Some(body_str) = body {
            req_builder = req_builder.body(body_str);
        }
    }

    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;

    let status = response.status().as_u16();
    let response_headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // 根据 Content-Type 决定如何处理响应体
    let content_type = response_headers.get("content-type")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    
    // 检查是否需要自动解压（gzip 或 br 压缩）
    let encoding = response_headers.get("content-encoding")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    
    // 读取响应体为 bytes
    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{}", e))?;
    
    // 自动解压压缩内容
    let decompressed_bytes = if encoding.contains("gzip") {
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut decoder = GzDecoder::new(&response_bytes[..]);
        let mut decompressed = Vec::new();
        let _ = decoder.read_to_end(&mut decompressed);
        if decompressed.is_empty() { response_bytes.to_vec() } else { decompressed }
    } else {
        response_bytes.to_vec()
    };
    
    // 判断是否为二进制内容
    let is_binary = content_type.starts_with("image/") 
        || content_type.starts_with("application/octet-stream")
        || content_type.starts_with("audio/")
        || content_type.starts_with("video/")
        || content_type.starts_with("application/pdf")
        || content_type.starts_with("application/zip");
    
    // 如果是图片或二进制数据，转为 base64
    let response_body = if is_binary {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&decompressed_bytes)
    } else {
        // 文本内容，尝试转为字符串
        String::from_utf8_lossy(&decompressed_bytes).to_string()
    };

    // 调试日志：显示返回内容的前 200 个字符（按字符截取，避免切断 UTF-8）
    if response_body.len() < 500 {
        log::info!("代理响应 [{}] body: {}", url, response_body);
    } else {
        let truncated: String = response_body.chars().take(200).collect();
        log::info!("代理响应 [{}] body: {}...", url, truncated);
    }

    Ok(CommandResponse::success(serde_json::json!({
        "status": status,
        "headers": response_headers,
        "body": response_body,
        "is_base64": is_binary,
        "encoding": if encoding.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(encoding) }
    })))
}
