use std::collections::HashMap;
use tauri::State;

use super::{extract_domain, CommandResponse, ProxyConfig};

/// 代理 fetch 请求 - 解决 CORS 问题
#[tauri::command]
pub async fn proxy_fetch(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    response_type: Option<String>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<serde_json::Value>, String> {
    use std::time::Instant;
    let start_time = Instant::now();

    // 清洗 body：去除 JSON 序列化带来的多余引号
    let body = body.map(|b| {
        let trimmed = b.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
            // 去掉外层引号并处理转义字符
            let inner = &trimmed[1..trimmed.len() - 1];
            inner
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
        } else {
            b
        }
    });

    log::info!("代理请求：{} {}", method, url);
    log::info!("Headers: {:?}", headers);

    let domain = extract_domain(&url);
    
    // 从数据库查询 cookies（使用全局 DB_CONN）
    let cookie_header = {
        let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
            db_mutex.lock().map_err(|e| e.to_string())?
        } else {
            return Err("DB not initialized".to_string());
        };
        
        let mut all_cookies = Vec::new();
        
        // 优先使用 WebView 同步的 cookies
        if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "webview", &domain) {
            for (k, v) in cookies {
                all_cookies.push(format!("{}={}", k, v));
            }
        }
        
        // 如果没有 WebView cookies，使用 browser 的
        if all_cookies.is_empty() {
            if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "browser", &domain) {
                for (k, v) in cookies {
                    all_cookies.push(format!("{}={}", k, v));
                }
            }
        }
        
        // 最后尝试 default
        if all_cookies.is_empty() {
            if let Ok(cookies) = crate::db::get_cookies_for_domain(&conn, "default", &domain) {
                for (k, v) in cookies {
                    all_cookies.push(format!("{}={}", k, v));
                }
            }
        }
        
        all_cookies.join("; ")
    };
    if !cookie_header.is_empty() {
        log::info!("使用 Cookies: {}", cookie_header);
    }

    // 从数据库读取全局 User-Agent
    let user_agent = if let Some(db_mutex) = crate::DB_CONN.get() {
        if let Ok(conn) = db_mutex.lock() {
            crate::db::get_user_agent(&conn).unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    let mut client_builder = reqwest::Client::builder().default_headers({
        let mut headers = reqwest::header::HeaderMap::new();
        if !cookie_header.is_empty() {
            headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
        }
        // 添加全局 User-Agent
        if !user_agent.is_empty() {
            headers.insert(reqwest::header::USER_AGENT, user_agent.parse().unwrap());
        }
        headers
    });

    log::info!("Body: {:?}", body);

    // 添加超时配置
    client_builder = client_builder
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10));

    let proxy = proxy_config.read().await;
    if let Some(proxy_settings) = proxy.as_ref() {
        if proxy_settings.enabled {
            let proxy_url = proxy_settings.get_proxy_url();
            log::info!("使用代理: {}", proxy_url);

            client_builder = client_builder.proxy(
                reqwest::Proxy::all(&proxy_url).map_err(|e| format!("代理配置失败：{}", e))?,
            );
        }
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
            let clean_body = &body_str[1..body_str.len() - 1];
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
    let _content_type = response_headers
        .get("content-type")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // 检查是否需要自动解压（gzip 或 br 压缩）
    let encoding = response_headers
        .get("content-encoding")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // 读取响应体为 bytes（带大小限制）
    log::info!("开始读取响应体...");
    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{}", e))?;

    // 限制最大响应大小 10MB
    const MAX_SIZE: usize = 10 * 1024 * 1024;
    if response_bytes.len() > MAX_SIZE {
        return Err(format!(
            "响应体过大：{} bytes (最大限制 10MB)",
            response_bytes.len()
        ));
    }

    // 自动解压压缩内容
    let decompressed_bytes = if encoding.contains("gzip") {
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut decoder = GzDecoder::new(&response_bytes[..]);
        let mut decompressed = Vec::new();
        let _ = decoder.read_to_end(&mut decompressed);
        if decompressed.is_empty() {
            response_bytes.to_vec()
        } else {
            decompressed
        }
    } else {
        response_bytes.to_vec()
    };

    // 获取 response_type，默认为 "text"
    let response_type = response_type
        .unwrap_or_else(|| "text".to_string())
        .to_lowercase();

    // 根据 response_type 处理响应体
    let (response_body, is_binary) = match response_type.as_str() {
        "arraybuffer" | "blob" => {
            // 返回 base64 编码的数据，前端可以转为 ArrayBuffer 或 Blob
            use base64::Engine;
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&decompressed_bytes);
            (base64_data, true)
        }
        "base64" => {
            // 纯 base64 字符串返回
            use base64::Engine;
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&decompressed_bytes);
            (base64_data, false)
        }
        "json" => {
            // JSON 格式，直接返回字符串，前端会解析
            let text = String::from_utf8_lossy(&decompressed_bytes).to_string();
            (text, false)
        }
        "text" | _ => {
            // 文本格式（默认）
            let text = String::from_utf8_lossy(&decompressed_bytes).to_string();
            (text, false)
        }
    };

    let elapsed = start_time.elapsed();

    // 调试日志：显示返回内容的前 200 个字符（按字符截取，避免切断 UTF-8）
    if response_body.len() < 500 {
        log::info!(
            "代理响应 [{}] body: {} (耗时: {:?})",
            url,
            response_body,
            elapsed
        );
    } else {
        let truncated: String = response_body.chars().take(200).collect();
        log::info!(
            "代理响应 [{}] body: {}... (耗时: {:?})",
            url,
            truncated,
            elapsed
        );
    }

    //log headers
    // let mut headers = response_headers.clone();
    // headers.insert("content-length".to_string(), response_body.len().to_string());
    // log::info!("代理响应 [{}] headers: {:?}", url, headers);
    Ok(CommandResponse::success(serde_json::json!({
        "status": status,
        "headers": response_headers,
        "body": response_body,
        "is_base64": is_binary,
        "response_type": response_type,
        "encoding": if encoding.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(encoding) }
    })))
}
