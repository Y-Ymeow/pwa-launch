use tauri::State;
use std::collections::HashMap;

use super::{CookieStore, ProxyConfig, CommandResponse, extract_domain};

/// 代理 fetch 请求 - 解决 CORS 问题
#[tauri::command]
pub async fn proxy_fetch(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    app_id: String,
    cookie_store: State<'_, CookieStore>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<serde_json::Value>, String> {
    log::info!("代理请求：{} {} (app: {})", method, url, app_id);

    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    let cookie_header = cookies.get(&app_id)
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| c.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("; "))
        .unwrap_or_default();
    drop(cookies);

    let mut client_builder = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            if !cookie_header.is_empty() {
                headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
            }
            headers
        });

    let proxy = proxy_config.read().await;
    if let Some(proxy_settings) = proxy.as_ref() {
        let proxy_url = if let (Some(user), Some(pass)) = (&proxy_settings.username, &proxy_settings.password) {
            format!("{}://{}:{}@{}",
                proxy_settings.url.split("://").next().unwrap_or("http"),
                user, pass,
                proxy_settings.url.split("://").last().unwrap_or(&proxy_settings.url))
        } else {
            proxy_settings.url.clone()
        };

        client_builder = client_builder.proxy(
            reqwest::Proxy::all(&proxy_url)
                .map_err(|e| format!("代理配置失败：{}", e))?
        );
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

    for (key, value) in headers {
        if key.to_lowercase() != "cookie" {
            req_builder = req_builder.header(&key, value);
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

    let response_body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{}", e))?;

    Ok(CommandResponse::success(serde_json::json!({
        "status": status,
        "headers": response_headers,
        "body": response_body
    })))
}
