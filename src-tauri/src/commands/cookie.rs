use std::collections::HashMap;
use tauri::State;

use super::{extract_domain, CommandResponse, CookieStore, ProxyConfig, ProxySettings};

/// 读取 Cookie - 按 app_id 隔离
#[tauri::command]
pub async fn get_cookies(
    url: String,
    app_id: String,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<Vec<String>>, String> {
    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    let result = cookies
        .get(&app_id)
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| c.iter().map(|(k, v)| format!("{}={}", k, v)).collect())
        .unwrap_or_default();
    Ok(CommandResponse::success(result))
}

/// 设置 Cookie - 按 app_id 隔离
#[tauri::command]
pub async fn set_cookies(
    url: String,
    app_id: String,
    cookies: Vec<String>,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<bool>, String> {
    let domain = extract_domain(&url);
    let mut store = cookie_store.write().await;
    let app_cookies = store.entry(app_id.clone()).or_insert_with(HashMap::new);
    let domain_cookies = app_cookies
        .entry(domain.clone())
        .or_insert_with(HashMap::new);

    for cookie in cookies {
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                domain_cookies.insert(key, value);
            }
        }
    }

    log::info!(
        "设置 Cookie (app: {}): {} {:?}",
        app_id,
        domain,
        domain_cookies
    );
    Ok(CommandResponse::success(true))
}

/// 清除指定 app 的 Cookie
#[tauri::command]
pub async fn clear_cookies(
    app_id: String,
    domain: Option<String>,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<bool>, String> {
    let mut store = cookie_store.write().await;
    if let Some(d) = domain {
        if let Some(app_cookies) = store.get_mut(&app_id) {
            app_cookies.remove(&d);
            log::info!("清除 Cookie (app: {}, domain: {})", app_id, d);
        }
    } else {
        store.remove(&app_id);
        log::info!("清除所有 Cookie (app: {})", app_id);
    }
    Ok(CommandResponse::success(true))
}

/// 获取指定 app 的所有 Cookie
#[tauri::command]
pub async fn get_all_cookies(
    app_id: String,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<HashMap<String, HashMap<String, String>>>, String> {
    let cookies = cookie_store.read().await;
    let result = cookies.get(&app_id).map(|c| c.clone()).unwrap_or_default();
    Ok(CommandResponse::success(result))
}

/// 设置代理
#[tauri::command]
pub async fn set_proxy(
    enabled: bool,
    proxy_type: super::ProxyType,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<bool>, String> {
    let proxy_settings = ProxySettings {
        enabled,
        proxy_type: proxy_type.clone(),
        host: host.clone(),
        port,
        username: username.clone(),
        password: password.clone(),
    };

    // 更新全局配置
    let mut config = proxy_config.write().await;
    *config = Some(proxy_settings.clone());
    drop(config);

    // 设置环境变量供 static_protocol 使用（同步上下文无法访问 State）
    if enabled {
        let proxy_url = proxy_settings.get_proxy_url();
        std::env::set_var("PWA_PROXY_URL", &proxy_url);
        log::info!("设置代理环境变量: {}", proxy_url);
    } else {
        std::env::remove_var("PWA_PROXY_URL");
        log::info!("清除代理环境变量");
    }

    log::info!("设置代理：{:?}", proxy_settings);
    Ok(CommandResponse::success(true))
}

/// 获取代理设置
#[tauri::command]
pub async fn get_proxy(
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<Option<ProxySettings>>, String> {
    let config = proxy_config.read().await;
    Ok(CommandResponse::success(config.clone()))
}

/// 禁用代理
#[tauri::command]
pub async fn disable_proxy(
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<bool>, String> {
    let mut config = proxy_config.write().await;
    if let Some(ref mut settings) = *config {
        settings.enabled = false;
        log::info!("禁用代理");
    }
    Ok(CommandResponse::success(true))
}

/// 从指定 WebView 获取 Cookies
#[tauri::command]
pub async fn get_webview_cookies(
    window: tauri::WebviewWindow,
    url: Option<String>,
) -> Result<CommandResponse<String>, String> {
    // 执行 JS 获取 document.cookie
    let script = r#"
        (function() {
            return document.cookie || '';
        })()
    "#;

    // 注意：Tauri 2.0 的 eval 返回 ()，需要通过其他方式获取返回值
    // 这里使用一个变通方案：设置一个全局变量，然后通过返回值获取
    window
        .eval(script)
        .map_err(|e| format!("执行 JS 失败: {:?}", e))?;

    // 由于 Tauri 2.0 eval 不返回 JS 执行结果，
    // 我们需要使用 event 或命令回调来获取结果
    // 简化处理：返回空，实际通过 sync_webview_cookies 来同步
    Ok(CommandResponse::success(String::new()))
}

/// 从 WebView 同步 Cookies（验证助手使用）
#[tauri::command]
pub async fn sync_webview_cookies(
    domain: String,
    cookies: String,
    user_agent: Option<String>,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("同步 WebView Cookies for domain: {}", domain);
    log::info!("User-Agent: {:?}", user_agent);

    let mut store = cookie_store.write().await;
    // 使用 "webview" 作为特殊的 app_id 来存储验证后的 cookies
    let app_cookies = store
        .entry("webview".to_string())
        .or_insert_with(HashMap::new);
    let domain_cookies = app_cookies
        .entry(domain.clone())
        .or_insert_with(HashMap::new);

    // 解析 cookies 字符串 (格式: "key1=value1; key2=value2")
    for cookie in cookies.split(';') {
        let cookie = cookie.trim();
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() {
                domain_cookies.insert(key, value);
            }
        }
    }

    log::info!(
        "WebView Cookies 同步完成: {} 个 cookies",
        domain_cookies.len()
    );
    Ok(CommandResponse::success(true))
}

/// 从代理服务器获取 Cookies（已废弃，本地服务器已移除）
#[tauri::command]
pub async fn get_proxy_cookies(
    _domain: Option<String>,
) -> Result<CommandResponse<serde_json::Value>, String> {
    // 本地服务器已移除，此命令不再使用
    Ok(CommandResponse::success(serde_json::json!({})))
}
