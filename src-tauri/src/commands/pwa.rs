use tauri::{State, Manager};
use rusqlite::Connection;
use std::collections::HashMap;

use crate::db::{DbConnection, get_app_data_dir};
use crate::models::{AppInfo, InstallRequest, CommandResponse};
use crate::utils::{generate_app_id, now_timestamp, create_app_dirs, remove_app_dirs};

use super::extract_domain;

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

    create_app_dirs(&app_id, &app_data_dir)
        .map_err(|e| format!("创建目录失败：{}", e))?;

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

    remove_app_dirs(&app_id, &app_data_dir)
        .map_err(|e| format!("删除目录失败：{}", e))?;

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

    let window_id = format!("pwa_{}_{}", app_id, chrono::Utc::now().timestamp());

    let (width, height, resizable, decorations, fullscreen) = match display_mode.as_str() {
        "fullscreen" => (1920, 1080, true, true, true),
        "minimal-ui" => (800, 600, false, true, false),
        "standalone" => (1200, 800, true, true, false),
        "window-controls-overlay" => (1200, 800, true, false, false),
        _ => (1200, 800, true, true, false),
    };

    let app_data_dir = app.path().app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    let user_data_dir = get_app_data_dir(&app_id, &app_data_dir);
    std::fs::create_dir_all(&user_data_dir).ok();

    #[cfg(not(mobile))]
    let window = {
        let mut builder = tauri::window::WindowBuilder::new(&app, &window_id)
            .title(&app_name)
            .inner_size(width as f64, height as f64)
            .resizable(resizable)
            .decorations(decorations);
        if fullscreen {
            builder = builder.fullscreen(fullscreen);
        }
        builder.build().map_err(|e| format!("创建窗口失败：{}", e))?
    };

    #[cfg(mobile)]
    let window = tauri::window::WindowBuilder::new(&app, &window_id)
        .build()
        .map_err(|e| format!("创建窗口失败：{}", e))?;

    let webview_builder = tauri::webview::WebviewBuilder::new(
        format!("{}_webview", window_id),
        tauri::WebviewUrl::External(app_url.parse().map_err(|e| format!("URL 解析失败：{}", e))?)
    )
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
    .devtools(true)
    .data_directory(user_data_dir)
    .additional_browser_args("--disable-web-security --disable-site-isolation-trials --allow-file-access-from-files --disable-features=SameSiteByDefaultCookies,CookiesWithoutSameSiteMustBeSecure,IsolateOrigins,site-per-process --disable-features=BlockInsecurePrivateNetworkRequests");

    let init_script = format!(r#"
        (function() {{
            window.__PWA_APP_ID__ = '{app_id}';
            console.log('[PWA Container] 注入脚本已加载，App ID:', window.__PWA_APP_ID__);

            const tauri = window.__TAURI_INTERNALS__ || window.__TAURI__;

            window.__PWA_PROXY__ = {{
                fetch: async function(url, init = {{}}) {{
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
                    const result = await tauri.invoke('proxy_fetch', {{
                        url, method, headers, body: body || null, appId: window.__PWA_APP_ID__
                    }});
                    if (result.success && result.data) {{
                        const {{ status, headers, body }} = result.data;
                        return new Response(body, {{ status, headers: new Headers(headers) }});
                    }}
                    throw new Error('代理请求失败：' + (result.error || '未知错误'));
                }}
            }};

            console.log('[PWA Container] 跨域代理工具已就绪');
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

/// 关闭 PWA 窗口
#[tauri::command]
pub fn close_pwa_window(
    window_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    if let Some(window) = app.get_webview_window(&window_id) {
        #[cfg(not(mobile))]
        window.destroy().map_err(|e| format!("关闭窗口失败：{}", e))?;
        #[cfg(mobile)]
        {
            // 移动端关闭窗口的处理方式不同
            let _ = window;
        }
        Ok(CommandResponse::success(true))
    } else {
        Ok(CommandResponse::success(false))
    }
}

/// 列出运行中的 PWA 窗口
#[tauri::command]
pub fn list_running_pwas(
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<String>>, String> {
    let windows = app.webview_windows();
    let ids: Vec<String> = windows.keys()
        .filter(|id| id.starts_with("pwa_"))
        .map(|id| id.clone())
        .collect();
    Ok(CommandResponse::success(ids))
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

    let html = client.get(url).send().await?.text().await?;
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

    if let Some(m_url) = manifest_url {
        if let Ok(manifest) = client.get(&m_url).send().await?.json::<serde_json::Value>().await {
            manifest_info.name = manifest["name"].as_str().or_else(|| manifest["short_name"].as_str()).map(String::from);
            manifest_info.start_url = manifest["start_url"].as_str().map(String::from);
            manifest_info.scope = manifest["scope"].as_str().map(String::from);
            manifest_info.theme_color = manifest["theme_color"].as_str().map(String::from);
            manifest_info.background_color = manifest["background_color"].as_str().map(String::from);
            manifest_info.display_mode = manifest["display"].as_str().map(String::from);

            if let Some(icons) = manifest["icons"].as_array() {
                if let Some(icon) = icons.first() {
                    manifest_info.icon_url = icon["src"].as_str().map(|src| absolute_url(src, url));
                }
            }
        }
    }

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
