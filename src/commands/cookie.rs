use tauri::State;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{CookieStore, ProxyConfig, ProxySettings, CommandResponse, extract_domain};

/// 读取 Cookie - 按 app_id 隔离
#[tauri::command]
pub async fn get_cookies(
    url: String,
    app_id: String,
    cookie_store: State<'_, CookieStore>,
) -> Result<CommandResponse<Vec<String>>, String> {
    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    let result = cookies.get(&app_id)
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
    let domain_cookies = app_cookies.entry(domain.clone()).or_insert_with(HashMap::new);

    for cookie in cookies {
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                domain_cookies.insert(key, value);
            }
        }
    }

    log::info!("设置 Cookie (app: {}): {} {:?}", app_id, domain, domain_cookies);
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
    let result = cookies.get(&app_id)
        .map(|c| c.clone())
        .unwrap_or_default();
    Ok(CommandResponse::success(result))
}

/// 设置代理
#[tauri::command]
pub async fn set_proxy(
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<bool>, String> {
    let mut config = proxy_config.write().await;
    *config = url.map(|u| ProxySettings { url: u, username, password });
    log::info!("设置代理：{:?}", *config);
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
    *config = None;
    log::info!("禁用代理");
    Ok(CommandResponse::success(true))
}
