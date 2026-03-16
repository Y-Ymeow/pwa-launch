use std::collections::HashMap;
use tauri::State;

use super::{extract_domain, CommandResponse, ProxyConfig, ProxySettings};

/// 读取 Cookie - 直接查数据库
#[tauri::command]
pub async fn get_cookies(
    url: String,
    app_id: String,
) -> Result<CommandResponse<Vec<String>>, String> {
    let domain = extract_domain(&url);
    
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    match crate::db::get_cookies_for_domain(&conn, &app_id, &domain) {
        Ok(cookies) => {
            let result: Vec<String> = cookies.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            Ok(CommandResponse::success(result))
        }
        Err(e) => {
            log::error!("[Cookies] Failed to get from DB: {}", e);
            Ok(CommandResponse::success(vec![]))
        }
    }
}

/// 设置 Cookie - 直接保存到数据库
#[tauri::command]
pub async fn set_cookies(
    url: String,
    app_id: String,
    cookies: Vec<String>,
) -> Result<CommandResponse<bool>, String> {
    let domain = extract_domain(&url);
    
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    for cookie in cookies {
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                if let Err(e) = crate::db::save_cookie(&conn, &app_id, &domain, &key, &value) {
                    log::error!("[Cookies] Failed to save cookie: {}", e);
                }
            }
        }
    }

    log::info!("[Cookies] Set cookies for app: {}, domain: {}", app_id, domain);
    Ok(CommandResponse::success(true))
}

/// 清除指定 app 的 Cookie（直接从数据库删除）
#[tauri::command]
pub async fn clear_cookies(
    app_id: String,
    domain: Option<String>,
    include_subdomains: Option<bool>,
) -> Result<CommandResponse<bool>, String> {
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };

    if let Some(d) = domain {
        let include_subs = include_subdomains.unwrap_or(true);
        if include_subs {
            // 清除该域名及其子域
            // 例如 domain=manhwa-raw.com 会清除:
            // - manhwa-raw.com
            // - www.manhwa-raw.com
            // - xxx.manhwa-raw.com
            let like_pattern = format!("%.{}", d);
            conn.execute(
                "DELETE FROM cookies WHERE app_id = ?1 AND (domain = ?2 OR domain LIKE ?3)",
                rusqlite::params![app_id, d, like_pattern],
            ).map_err(|e| e.to_string())?;
            log::info!("清除 Cookie (app: {}, domain: {} 及子域)", app_id, d);
        } else {
            conn.execute(
                "DELETE FROM cookies WHERE app_id = ?1 AND domain = ?2",
                rusqlite::params![app_id, d],
            ).map_err(|e| e.to_string())?;
            log::info!("清除 Cookie (app: {}, domain: {})", app_id, d);
        }
    } else {
        conn.execute(
            "DELETE FROM cookies WHERE app_id = ?1",
            rusqlite::params![app_id],
        ).map_err(|e| e.to_string())?;
        log::info!("清除所有 Cookie (app: {})", app_id);
    }
    Ok(CommandResponse::success(true))
}

/// 获取所有有 cookies 的域名列表
#[tauri::command]
pub async fn get_cookie_domains() -> Result<CommandResponse<Vec<String>>, String> {
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    match crate::db::get_cookie_domains(&conn) {
        Ok(domains) => {
            log::info!("[Cookies] Found {} domains", domains.len());
            Ok(CommandResponse::success(domains))
        }
        Err(e) => Err(format!("查询失败: {}", e)),
    }
}

/// 获取指定 app 的所有 Cookie（直接从数据库查询）
#[tauri::command]
pub async fn get_all_cookies(
    app_id: String,
) -> Result<CommandResponse<HashMap<String, HashMap<String, String>>>, String> {
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT domain, name, value FROM cookies WHERE app_id = ?1"
    ).map_err(|e| e.to_string())?;
    
    let rows = stmt.query_map(rusqlite::params![app_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }).map_err(|e| e.to_string())?;
    
    for row in rows {
        let (domain, name, value) = row.map_err(|e| e.to_string())?;
        let domain_cookies = result.entry(domain).or_insert_with(HashMap::new);
        domain_cookies.insert(name, value);
    }
    
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

/// 从指定 WebView 获取 Cookies（包括 HttpOnly）
#[tauri::command]
pub fn get_webview_cookies(
    window: tauri::WebviewWindow,
) -> Result<CommandResponse<String>, String> {
    // 使用 WebView 的 cookies() API 获取所有 cookies（包括 HttpOnly）
    let cookies = window.cookies()
        .map_err(|e| format!("获取 cookies 失败: {:?}", e))?;
    
    // 将 cookies 转换为字符串格式
    let cookie_str = cookies.iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
    
    log::info!("从 WebView 获取到 {} 个 cookies", cookies.len());
    Ok(CommandResponse::success(cookie_str))
}

/// 从 WebView 同步 Cookies（直接保存到数据库）
#[tauri::command]
pub async fn sync_webview_cookies(
    domain: String,
    cookies: String,
    user_agent: Option<String>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("同步 WebView Cookies for domain: {}", domain);
    log::info!("User-Agent: {:?}", user_agent);

    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    let mut count = 0;

    // 解析 cookies 字符串 (格式: "key1=value1; key2=value2")
    for cookie in cookies.split(';') {
        let cookie = cookie.trim();
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() {
                if let Err(e) = crate::db::save_cookie(&conn, "webview", &domain, &key, &value) {
                    log::error!("[Cookies] Failed to save: {}", e);
                } else {
                    count += 1;
                }
            }
        }
    }

    log::info!("WebView Cookies 同步完成: {} 个 cookies", count);
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
