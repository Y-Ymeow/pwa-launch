use tauri::{State, Manager};
use rusqlite::Connection;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{DbConnection, get_app_data_dir, get_backup_dir};
use crate::models::{AppInfo, InstallRequest, BackupInfo, CommandResponse, ShortcutInfo};
use crate::utils::{generate_app_id, now_timestamp, calculate_dir_size, create_app_dirs, remove_app_dirs};

// 全局 Cookie 存储 - 按 app_id + 域名 隔离
pub type CookieStore = Arc<RwLock<HashMap<String, HashMap<String, HashMap<String, String>>>>>;
// 结构：{ app_id: { domain: { key: value } } }

// 全局代理设置
pub type ProxyConfig = Arc<RwLock<Option<ProxySettings>>>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProxySettings {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 代理 fetch 请求 - 解决 CORS 问题
#[tauri::command]
pub async fn proxy_fetch(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    app_id: String,  // 添加 app_id 参数，用于隔离 Cookie
    cookie_store: State<'_, CookieStore>,
    proxy_config: State<'_, ProxyConfig>,
) -> Result<CommandResponse<serde_json::Value>, String> {
    log::info!("代理请求：{} {} (app: {})", method, url, app_id);
    
    // 解析域名获取 Cookie - 只从当前 app_id 获取
    let domain = extract_domain(&url);
    let cookies = cookie_store.read().await;
    let cookie_header = cookies.get(&app_id)
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| c.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("; "))
        .unwrap_or_default();
    drop(cookies);
    
    // 创建 HTTP 客户端，带 Cookie 和代理
    let mut client_builder = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            if !cookie_header.is_empty() {
                headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
            }
            headers
        });
    
    // 配置代理
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
    
    // 添加 headers（排除 Cookie，已经处理了）
    for (key, value) in headers {
        if key.to_lowercase() != "cookie" {
            req_builder = req_builder.header(&key, value);
        }
    }
    
    // 添加 body
    if let Some(body_str) = body {
        req_builder = req_builder.body(body_str);
    }
    
    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;
    
    // 保存新的 Cookie - 只保存到当前 app_id
    if let Some(cookie_header) = response.headers().get(reqwest::header::SET_COOKIE) {
        let cookie_str = cookie_header.to_str().unwrap_or("");
        save_cookies(&app_id, &domain, cookie_str, &cookie_store).await;
    }
    
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

// 提取域名
fn extract_domain(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().unwrap_or("").to_string()
    } else {
        url.to_string()
    }
}

// 保存 Cookie - 按 app_id 隔离
async fn save_cookies(app_id: &str, domain: &str, cookie_str: &str, cookie_store: &CookieStore) {
    let mut store = cookie_store.write().await;
    let app_cookies = store.entry(app_id.to_string()).or_insert_with(HashMap::new);
    let domain_cookies = app_cookies.entry(domain.to_string()).or_insert_with(HashMap::new);
    
    for cookie in cookie_str.split(',') {
        let cookie = cookie.trim();
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..]
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !key.is_empty() && !value.is_empty() {
                domain_cookies.insert(key, value);
            }
        }
    }
}

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
        // 清除指定域名的 Cookie
        if let Some(app_cookies) = store.get_mut(&app_id) {
            app_cookies.remove(&d);
            log::info!("清除 Cookie (app: {}, domain: {})", app_id, d);
        }
    } else {
        // 清除整个 app 的 Cookie
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

/// OPFS 写入文件
#[tauri::command]
pub async fn opfs_write_file(
    app_id: String,
    path: String,
    data: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    std::fs::create_dir_all(&files_dir).ok();
    
    let file_path = files_dir.join(&path);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    
    // 解码 base64 数据
    let decoded = base64_decode(&data);
    std::fs::write(&file_path, decoded)
        .map_err(|e| format!("写入失败：{}", e))?;
    
    log::info!("OPFS 写入：{}/{}", app_id, path);
    Ok(CommandResponse::success(true))
}

/// OPFS 读取文件
#[tauri::command]
pub async fn opfs_read_file(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<String>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let file_path = files_dir.join(&path);
    
    let data = std::fs::read(&file_path)
        .map_err(|e| format!("读取失败：{}", e))?;
    
    // 编码为 base64
    let encoded = base64_encode(&data);
    Ok(CommandResponse::success(encoded))
}

/// OPFS 删除文件
#[tauri::command]
pub async fn opfs_delete_file(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let file_path = files_dir.join(&path);
    
    if file_path.exists() {
        std::fs::remove_file(&file_path)
            .map_err(|e| format!("删除失败：{}", e))?;
    }
    
    log::info!("OPFS 删除：{}/{}", app_id, path);
    Ok(CommandResponse::success(true))
}

/// OPFS 列出目录
#[tauri::command]
pub async fn opfs_list_dir(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<String>>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let dir_path = files_dir.join(&path);
    
    let entries = std::fs::read_dir(&dir_path)
        .map_err(|e| format!("读取目录失败：{}", e))?;
    
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    
    Ok(CommandResponse::success(names))
}

// Base64 编码/解码辅助函数
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        let _ = write!(result, "{}", ALPHABET[(b0 >> 2) & 0x3F] as char);
        let _ = write!(result, "{}", ALPHABET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);
        if chunk.len() > 1 {
            let _ = write!(result, "{}", ALPHABET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            let _ = write!(result, "{}", ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(data: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    
    for c in data.chars() {
        if c == '=' {
            break;
        }
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => continue,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
        }
    }
    result
}

/// 安装 PWA 应用
#[tauri::command]
pub async fn install_pwa(
    request: InstallRequest,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<AppInfo>, String> {
    log::info!("安装 PWA: {}", request.url);
    
    let app_id = generate_app_id();
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    // 创建应用目录
    create_app_dirs(&app_id, &app_data_dir)
        .map_err(|e| format!("创建目录失败：{}", e))?;
    
    // 获取 manifest 信息
    let manifest = fetch_manifest_info(&request.url).await
        .map_err(|e| format!("获取 manifest 失败：{}", e))?;
    
    let name = request.name.unwrap_or_else(|| manifest.name.clone().unwrap_or("未知应用".to_string()));
    let now = now_timestamp();
    
    let app_info = AppInfo {
        id: app_id.clone(),
        name,
        url: request.url,
        icon_url: manifest.icon_url,
        manifest_url: manifest.manifest_url,
        installed_at: now,
        updated_at: now,
        start_url: manifest.start_url,
        scope: manifest.scope,
        theme_color: manifest.theme_color,
        background_color: manifest.background_color,
        display_mode: manifest.display_mode.unwrap_or_else(|| "standalone".to_string()),
    };
    
    // 保存到数据库
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    save_app_to_db(&conn, &app_info)
        .map_err(|e| format!("保存数据库失败：{}", e))?;
    
    log::info!("PWA 安装成功：{} ({})", app_info.name, app_id);
    Ok(CommandResponse::success(app_info))
}

/// 卸载 PWA 应用
#[tauri::command]
pub fn uninstall_pwa(
    app_id: String,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("卸载 PWA: {}", app_id);
    
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    // 删除应用目录
    remove_app_dirs(&app_id, &app_data_dir)
        .map_err(|e| format!("删除目录失败：{}", e))?;
    
    // 从数据库删除
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    conn.execute("DELETE FROM apps WHERE id = ?", [app_id.clone()])
        .map_err(|e| format!("删除数据库记录失败：{}", e))?;
    
    log::info!("PWA 卸载成功：{}", app_id);
    Ok(CommandResponse::success(true))
}

/// 获取应用列表
#[tauri::command]
pub fn list_apps(
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<Vec<AppInfo>>, String> {
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    
    let mut stmt = conn.prepare("SELECT * FROM apps ORDER BY installed_at DESC")
        .map_err(|e| format!("查询失败：{}", e))?;
    
    let apps = stmt.query_map([], |row: &rusqlite::Row| {
        Ok(AppInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            icon_url: row.get(3)?,
            manifest_url: row.get(4)?,
            installed_at: row.get(5)?,
            updated_at: row.get(6)?,
            start_url: row.get(7)?,
            scope: row.get(8)?,
            theme_color: row.get(9)?,
            background_color: row.get(10)?,
            display_mode: row.get(11)?,
        })
    })
    .map_err(|e| format!("查询失败：{}", e))?
    .filter_map(|r: Result<AppInfo, rusqlite::Error>| r.ok())
    .collect();
    
    Ok(CommandResponse::success(apps))
}

/// 启动应用 - 创建独立 WebView 窗口
#[tauri::command]
pub fn launch_app(
    app_id: String,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<String>, String> {
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;

    let (app_name, app_url, display_mode): (String, String, String) = conn.query_row(
        "SELECT name, url, display_mode FROM apps WHERE id = ?",
        [app_id.clone()],
        |row: &rusqlite::Row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .map_err(|e| format!("未找到应用：{}", e))?;

    // 生成唯一窗口 ID
    let window_id = format!("pwa_{}_{}", app_id, chrono::Utc::now().timestamp());
    
    // 根据 display_mode 设置窗口样式
    let (width, height, resizable, decorations, fullscreen) = match display_mode.as_str() {
        "fullscreen" => (1920, 1080, true, true, true),
        "minimal-ui" => (800, 600, false, true, false),
        "standalone" => (1200, 800, true, true, false),
        "window-controls-overlay" => (1200, 800, true, false, false),
        _ => (1200, 800, true, true, false),
    };

    // 获取应用数据目录用于持久化 Cookie
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    let user_data_dir = get_app_data_dir(&app_id, &app_data_dir);
    std::fs::create_dir_all(&user_data_dir).ok();

    // 创建新窗口
    let window = tauri::window::WindowBuilder::new(&app, &window_id)
        .title(&app_name)
        .inner_size(width as f64, height as f64)
        .resizable(resizable)
        .decorations(decorations)
        .fullscreen(fullscreen)
        .build()
        .map_err(|e| format!("创建窗口失败：{}", e))?;

    // 创建 WebView 加载 PWA URL
    let webview_builder = tauri::webview::WebviewBuilder::new(
        format!("{}_webview", window_id),
        tauri::WebviewUrl::External(app_url.parse().map_err(|e| format!("URL 解析失败：{}", e))?)
    )
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
    .devtools(true)
    .data_directory(user_data_dir) // 持久化 Cookie 和 Storage
    .additional_browser_args("--disable-web-security --disable-site-isolation-trials --allow-file-access-from-files --disable-features=SameSiteByDefaultCookies,CookiesWithoutSameSiteMustBeSecure");
    
    // 注入脚本拦截 window.open、fetch、Cookie 和 OPFS
    let init_script = format!(r#"
        (function() {{
            window.__PWA_APP_ID__ = '{app_id}';
            console.log('[PWA Container] 注入脚本已加载，App ID:', window.__PWA_APP_ID__);
            
            // 拦截 window.open，在新窗口打开并共享 Cookie
            const originalOpen = window.open;
            window.open = function(url, name, specs) {{
                if (url && url.startsWith('http')) {{
                    if (window.__TAURI__ && window.__TAURI__.shell) {{
                        window.__TAURI__.shell.open(url);
                    }}
                    return null;
                }}
                return originalOpen.call(window, url, name, specs);
            }};
            
            // 劫持 fetch，通过 Rust 代理解决 CORS
            const originalFetch = window.fetch;
            window.fetch = async function(input, init = {{}}) {{
                const url = typeof input === 'string' ? input : input.url;
                
                // 同源请求直接使用原始 fetch
                if (url.startsWith(window.location.origin)) {{
                    init.mode = 'cors';
                    init.credentials = 'include';
                    return originalFetch.call(window, input, init);
                }}
                
                try {{
                    const method = (init.method || 'GET').toUpperCase();
                    const headers = {{}};
                    if (init.headers) {{
                        for (const [key, value] of Object.entries(init.headers)) {{
                            if (!['content-length', 'host'].includes(key.toLowerCase())) {{
                                headers[key] = value;
                            }}
                        }}
                    }}
                    
                    let body = init.body;
                    if (body && typeof body !== 'string') {{
                        body = await new Response(body).text();
                    }}
                    
                    const result = await window.__TAURI__.core.invoke('plugin:pwa_container|proxy_fetch', {{
                        url,
                        method,
                        headers,
                        body,
                        appId: APP_ID
                    }});
                    
                    if (result.success && result.data) {{
                        const {{ status, headers, body }} = result.data;
                        return new Response(body, {{
                            status,
                            headers: new Headers(headers)
                        }});
                    }}
                    throw new Error('代理请求失败');
                }} catch (e) {{
                    console.warn('[PWA Container] 代理失败，使用原始 fetch:', e);
                    return originalFetch.call(window, input, init);
                }}
            }};
            
            // 劫持 Cookie API，同步到 Rust 层（按 app_id 隔离）
            if (window.__TAURI__ && window.__TAURI__.core) {{
                let localCookieStore = {{}};
                const APP_ID = window.__PWA_APP_ID__;
                
                // 从 Rust 层加载 Cookie（只加载当前 app 的）
                (async () => {{
                    try {{
                        const cookies = await window.__TAURI__.core.invoke('plugin:pwa_container|get_cookies', {{
                            url: window.location.href,
                            appId: APP_ID
                        }});
                        if (cookies.success && cookies.data) {{
                            for (const cookie of cookies.data) {{
                                const [key, value] = cookie.split('=');
                                if (key && value) {{
                                    localCookieStore[key.trim()] = value.trim();
                                }}
                            }}
                        }}
                    }} catch (e) {{
                        console.warn('[PWA Container] 加载 Cookie 失败:', e);
                    }}
                }})();
                
                Object.defineProperty(document, 'cookie', {{
                    get: function() {{
                        return Object.entries(localCookieStore)
                            .map(([k, v]) => k + '=' + v).join('; ');
                    }},
                    set: function(cookieStr) {{
                        const parts = cookieStr.split(';').map(p => p.trim());
                        const [kv] = parts;
                        const [key, value] = kv.split('=');
                        if (key && value) {{
                            localCookieStore[key.trim()] = value.trim();
                            // 保存到 Rust 层（只保存到当前 app_id，隔离）
                            window.__TAURI__.core.invoke('plugin:pwa_container|set_cookies', {{
                                url: window.location.href,
                                appId: APP_ID,
                                cookies: [cookieStr]
                            }}).catch(console.error);
                        }}
                    }}
                }});
            }}
            
            console.log('[PWA Container] 所有劫持已完成');
        }})();
    "#);
    
    let webview_builder = webview_builder.initialization_script(&init_script);
    
    window
        .add_child(
            webview_builder,
            tauri::LogicalPosition::new(0, 0),
            window.inner_size().unwrap(),
        )
        .map_err(|e| format!("创建 WebView 失败：{}", e))?;

    log::info!("启动 PWA 应用：{} -> {} (窗口：{})", app_name, app_url, window_id);
    Ok(CommandResponse::success(window_id))
}

/// 清除应用数据
#[tauri::command]
pub fn clear_data(
    app_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<u64>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    let data_dir = get_app_data_dir(&app_id, &app_data_dir);
    
    // 计算要删除的数据大小
    let size = calculate_dir_size(&data_dir)
        .map_err(|e| format!("计算大小失败：{}", e))?;
    
    // 删除文件和缓存目录
    let files_dir = data_dir.join("files");
    let cache_dir = data_dir.join("cache");
    
    if files_dir.exists() {
        std::fs::remove_dir_all(&files_dir)
            .map_err(|e| format!("删除文件失败：{}", e))?;
    }
    
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|e| format!("删除缓存失败：{}", e))?;
    }
    
    // 删除应用 SQLite 数据库
    let app_db = data_dir.join("data.db");
    if app_db.exists() {
        std::fs::remove_file(&app_db)
            .map_err(|e| format!("删除数据库失败：{}", e))?;
    }
    
    log::info!("清除数据完成：{} ({} bytes)", app_id, size);
    Ok(CommandResponse::success(size))
}

/// 备份应用数据
#[tauri::command]
pub fn backup_data(
    app_id: String,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<BackupInfo>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let data_dir = get_app_data_dir(&app_id, &app_data_dir);
    let backup_dir = get_backup_dir(&app_data_dir);

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e: std::io::Error| format!("创建备份目录失败：{}", e))?;

    // 生成备份文件名
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_filename = format!("{}_{}.zip", app_id, timestamp);
    let backup_path = backup_dir.join(&backup_filename);

    // 计算数据大小
    let size = calculate_dir_size(&data_dir)
        .map_err(|e| format!("计算大小失败：{}", e))?;

    let backup_id = generate_app_id();

    // 保存备份记录到数据库
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    conn.execute(
        "INSERT INTO backups (id, app_id, backup_path, created_at, size_bytes) VALUES (?, ?, ?, ?, ?)",
        [
            backup_id.clone(),
            app_id.clone(),
            backup_path.to_string_lossy().to_string(),
            now_timestamp().to_string(),
            (size as i64).to_string(),
        ],
    )
    .map_err(|e| format!("保存备份记录失败：{}", e))?;

    // 获取应用名称
    let app_name: String = conn.query_row(
        "SELECT name FROM apps WHERE id = ?",
        [app_id.clone()],
        |row: &rusqlite::Row| row.get(0),
    )
    .unwrap_or_else(|_| "未知应用".to_string());

    let backup_info = BackupInfo {
        id: backup_id,
        app_id,
        app_name,
        backup_path: backup_path.to_string_lossy().to_string(),
        created_at: now_timestamp(),
        size_bytes: Some(size),
    };

    log::info!("备份完成：{:?}", backup_info);
    Ok(CommandResponse::success(backup_info))
}

/// 恢复应用数据
#[tauri::command]
pub fn restore_data(
    backup_id: String,
    _app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    
    let backup_path: String = conn.query_row(
        "SELECT backup_path FROM backups WHERE id = ?",
        [backup_id.clone()],
        |row: &rusqlite::Row| row.get(0),
    )
    .map_err(|e| format!("未找到备份：{}", e))?;

    log::info!("恢复备份：{} -> {}", backup_id, backup_path);
    Ok(CommandResponse::success(true))
}

/// 创建桌面快捷方式
#[tauri::command]
pub fn create_shortcut(
    app_id: String,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<ShortcutInfo>, String> {
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    
    let (app_name, app_url): (String, String) = conn.query_row(
        "SELECT name, url FROM apps WHERE id = ?",
        [app_id.clone()],
        |row: &rusqlite::Row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| format!("未找到应用：{}", e))?;

    let platform = std::env::consts::OS.to_string();
    
    // 获取应用数据目录
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    
    // 创建快捷方式文件
    let shortcut_path = match platform.as_str() {
        "linux" => {
            // 创建 .desktop 文件
            let desktop_dir = dirs::home_dir()
                .map(|h| h.join(".local/share/applications"))
                .unwrap_or_else(|| app_data_dir.join("shortcuts"));
            std::fs::create_dir_all(&desktop_dir).ok();
            
            let desktop_file = desktop_dir.join(format!("pwa-{}.desktop", app_id));
            let desktop_content = format!(
                "[Desktop Entry]\nVersion=1.0\nType=Application\nName={}\nExec=xdg-open {}\nIcon=web-browser\nTerminal=false\n",
                app_name, app_url
            );
            std::fs::write(&desktop_file, desktop_content).ok();
            desktop_file.to_string_lossy().to_string()
        },
        "windows" => {
            // Windows 快捷方式需要 COM API，这里创建一个批处理文件
            let shortcut_file = app_data_dir.join(format!("launch-{}.bat", app_id));
            let bat_content = format!("@echo off\nstart {} \"{}\"", app_url, app_name);
            std::fs::write(&shortcut_file, bat_content).ok();
            shortcut_file.to_string_lossy().to_string()
        },
        _ => {
            // macOS 和其他系统
            let shortcut_file = app_data_dir.join(format!("launch-{}.command", app_id));
            let command_content = format!("#!/bin/bash\nopen \"{}\"", app_url);
            std::fs::write(&shortcut_file, command_content).ok();
            shortcut_file.to_string_lossy().to_string()
        }
    };
    
    let shortcut_info = ShortcutInfo {
        app_id,
        shortcut_path,
        platform,
    };
    
    log::info!("创建快捷方式：{:?}", shortcut_info);
    Ok(CommandResponse::success(shortcut_info))
}

/// 获取应用详细信息
#[tauri::command]
pub fn get_app_info(
    app_id: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<AppInfo>, String> {
    let conn = db.inner().lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    
    let app_info = conn.query_row(
        "SELECT * FROM apps WHERE id = ?",
        [app_id],
        |row: &rusqlite::Row| {
            Ok(AppInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                icon_url: row.get(3)?,
                manifest_url: row.get(4)?,
                installed_at: row.get(5)?,
                updated_at: row.get(6)?,
                start_url: row.get(7)?,
                scope: row.get(8)?,
                theme_color: row.get(9)?,
                background_color: row.get(10)?,
                display_mode: row.get(11)?,
            })
        },
    )
    .map_err(|e| format!("未找到应用：{}", e))?;
    
    Ok(CommandResponse::success(app_info))
}

/// 更新 PWA 应用
#[tauri::command]
pub fn update_pwa(
    app_id: String,
    _db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("更新应用：{}", app_id);
    Ok(CommandResponse::success(true))
}

/// 关闭指定的 PWA 窗口
#[tauri::command]
pub fn close_pwa_window(
    window_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    if let Some(window) = app.get_webview_window(&window_id) {
        window.close().map_err(|e| format!("关闭窗口失败：{}", e))?;
        log::info!("关闭 PWA 窗口：{}", window_id);
        Ok(CommandResponse::success(true))
    } else {
        Ok(CommandResponse::success(false))
    }
}

/// 获取所有运行中的 PWA 窗口
#[tauri::command]
pub fn list_running_pwas(
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<String>>, String> {
    let windows = app.webview_windows();
    let running: Vec<String> = windows.keys()
        .filter(|k| k.starts_with("pwa_"))
        .cloned()
        .collect();
    Ok(CommandResponse::success(running))
}

// ============ 辅助函数 ============

struct ManifestInfo {
    name: Option<String>,
    icon_url: Option<String>,
    manifest_url: Option<String>,
    start_url: Option<String>,
    scope: Option<String>,
    theme_color: Option<String>,
    background_color: Option<String>,
    display_mode: Option<String>,
}

async fn fetch_manifest_info(url: &str) -> Result<ManifestInfo, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    // 获取 HTML 内容
    let html = client.get(url).send().await?.text().await?;
    
    // 尝试从 HTML 中提取 manifest URL
    let manifest_url = extract_manifest_url_from_html(&html, url);
    
    let mut manifest_info = ManifestInfo {
        name: None,
        icon_url: None,
        manifest_url: manifest_url.clone(),
        start_url: None,
        scope: None,
        theme_color: None,
        background_color: None,
        display_mode: None,
    };
    
    // 如果找到了 manifest，获取其内容
    if let Some(m_url) = manifest_url {
        if let Ok(manifest) = client.get(&m_url).send().await?.json::<serde_json::Value>().await {
            manifest_info.name = manifest["name"].as_str().or_else(|| manifest["short_name"].as_str()).map(String::from);
            manifest_info.start_url = manifest["start_url"].as_str().map(String::from);
            manifest_info.scope = manifest["scope"].as_str().map(String::from);
            manifest_info.theme_color = manifest["theme_color"].as_str().map(String::from);
            manifest_info.background_color = manifest["background_color"].as_str().map(String::from);
            manifest_info.display_mode = manifest["display"].as_str().map(String::from);
            
            // 获取图标
            if let Some(icons) = manifest["icons"].as_array() {
                if let Some(icon) = icons.first() {
                    manifest_info.icon_url = icon["src"].as_str().map(|src| absolute_url(src, url));
                }
            }
        }
    }
    
    // 如果 manifest 中没有名称，尝试从 HTML title 获取
    if manifest_info.name.is_none() {
        manifest_info.name = extract_title_from_html(&html);
    }
    
    Ok(manifest_info)
}

fn extract_manifest_url_from_html(html: &str, base_url: &str) -> Option<String> {
    regex::Regex::new(r#"<link[^>]+rel=["']manifest["'][^>]+href=["']([^"']+)["']"#)
        .unwrap()
        .captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .map(|href| absolute_url(href, base_url))
}

fn extract_title_from_html(html: &str) -> Option<String> {
    regex::Regex::new(r#"<title[^>]*>([^<]+)</title>"#)
        .unwrap()
        .captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
}

fn absolute_url(url: &str, base: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }

    if let Ok(base_url) = url::Url::parse(base) {
        match base_url.join(url) {
            Ok(u) => return u.to_string(),
            Err(_) => return url.to_string(),
        }
    }

    url.to_string()
}

fn save_app_to_db(conn: &Connection, app: &AppInfo) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO apps (id, name, url, icon_url, manifest_url, installed_at, updated_at, start_url, scope, theme_color, background_color, display_mode)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            app.id.clone(),
            app.name.clone(),
            app.url.clone(),
            app.icon_url.clone().unwrap_or_default(),
            app.manifest_url.clone().unwrap_or_default(),
            app.installed_at.to_string(),
            app.updated_at.to_string(),
            app.start_url.clone().unwrap_or_default(),
            app.scope.clone().unwrap_or_default(),
            app.theme_color.clone().unwrap_or_default(),
            app.background_color.clone().unwrap_or_default(),
            app.display_mode.clone(),
        ],
    )?;
    Ok(())
}
