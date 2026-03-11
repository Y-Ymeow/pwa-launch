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
    log::info!("代理请求：{} {}", method, url);

    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    let cookie_header = cookies
        .get("default")
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| {
            c.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    drop(cookies);

    let mut client_builder = reqwest::Client::builder().default_headers({
        let mut headers = reqwest::header::HeaderMap::new();
        if !cookie_header.is_empty() {
            headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
        }
        headers
    });

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

    let mut req_builder = match method.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        "HEAD" => client.head(&url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, &url),
        _ => client.get(&url),
    };

    // 检查是否有自定义 Referer
    let has_referer = headers.keys().any(|k| k.to_lowercase() == "referer");
    
    for (key, value) in headers {
        if key.to_lowercase() != "cookie" {
            req_builder = req_builder.header(&key, value);
        }
    }
    
    // 如果没有 Referer，自动添加目标域名（避免防盗链）
    if !has_referer {
        let domain = extract_domain(&url);
        if !domain.is_empty() {
            req_builder = req_builder.header("Referer", format!("https://{}/", domain));
        }
    }

    if let Some(body_str) = body {
        req_builder = req_builder.body(body_str);
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
    
    // 读取响应体为 bytes
    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{}", e))?;
    
    // 如果是图片或二进制数据，转为 base64
    let response_body = if content_type.starts_with("image/") 
        || content_type.starts_with("application/octet-stream")
        || content_type.starts_with("audio/")
        || content_type.starts_with("video/") {
        // base64 编码
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&response_bytes)
    } else {
        // 文本内容，尝试转为字符串
        String::from_utf8_lossy(&response_bytes).to_string()
    };

    Ok(CommandResponse::success(serde_json::json!({
        "status": status,
        "headers": response_headers,
        "body": response_body,
        "is_base64": content_type.starts_with("image/") 
            || content_type.starts_with("application/octet-stream")
    })))
}
