use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, State};

use crate::db::{get_app_data_dir, DbConnection};
use crate::models::{AppInfo, CommandResponse, InstallRequest};
use crate::utils::{create_app_dirs, generate_app_id, now_timestamp};

/// 安装 PWA 应用
#[tauri::command]
pub async fn install_pwa(
    request: InstallRequest,
    app: AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<AppInfo>, String> {
    let app_id = generate_app_id();
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    create_app_dirs(&app_id, &app_data_dir).map_err(|e| e.to_string())?;

    let manifest = fetch_manifest_info(&request.url)
        .await
        .map_err(|e| format!("解析 Manifest 失败: {}", e))?;

    let name = request
        .name
        .unwrap_or_else(|| manifest.name.clone().unwrap_or("未知应用".to_string()));

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
        display_mode: manifest
            .display_mode
            .unwrap_or_else(|| "standalone".to_string()),
    };

    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    save_app_to_db(&conn, &app_info).map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(app_info))
}

/// 卸载 PWA 应用
#[tauri::command]
pub fn uninstall_pwa(
    app_id: String,
    _app: AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM apps WHERE id = ?", [app_id])
        .map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(true))
}

/// 获取应用列表
#[tauri::command]
pub fn list_apps(db: State<'_, DbConnection>) -> Result<CommandResponse<Vec<AppInfo>>, String> {
    let conn = db
        .inner()
        .lock()
        .map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM apps ORDER BY installed_at DESC")
        .map_err(|e| e.to_string())?;
    let apps = stmt
        .query_map([], |row| {
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
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(CommandResponse::success(apps))
}

/// 启动应用
#[tauri::command]
pub fn launch_app(
    app_id: String,
    app: AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<String>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    let (_app_name, app_url, _display_mode): (String, String, String) = conn
        .query_row(
            "SELECT name, url, display_mode FROM apps WHERE id = ?",
            [app_id.clone()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| format!("未找到应用：{}", e))?;

    #[cfg(not(mobile))]
    {
        let window_id = format!("pwa_{}", app_id);
        if let Some(window) = app.get_webview_window(&window_id) {
            window.set_focus().ok();
            return Ok(CommandResponse::success(window_id));
        }

        // 直接使用原始 URL
        let pwa_url = app_url.clone();

        let window = tauri::window::WindowBuilder::new(&app, &window_id)
            .title(&_app_name)
            .inner_size(1200.0, 800.0)
            .build()
            .map_err(|e| e.to_string())?;

        let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let user_data_dir = get_app_data_dir(&app_id, &app_data_dir);

        // 编译时嵌入 adapt.min.js 内容
        const ADAPT_JS: &str = include_str!("../../../adapt.min.js");

        let webview_builder = tauri::webview::WebviewBuilder::new(
            format!("{}_webview", window_id),
            tauri::WebviewUrl::External(
                pwa_url
                    .parse()
                    .map_err(|e: url::ParseError| e.to_string())?,
            ),
        )
        .data_directory(user_data_dir)
        .devtools(true)
        .initialization_script(ADAPT_JS);

        window
            .add_child(
                webview_builder,
                tauri::LogicalPosition::new(0, 0),
                window.inner_size().unwrap(),
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(CommandResponse::success(app_id))
}

#[tauri::command]
pub fn get_app_info(
    app_id: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<AppInfo>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    let app_info = conn
        .query_row("SELECT * FROM apps WHERE id = ?", [app_id], |row| {
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
        .map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(app_info))
}

#[tauri::command]
pub fn list_running_pwas(app: AppHandle) -> Result<CommandResponse<Vec<String>>, String> {
    let windows = app.webview_windows();
    let ids: Vec<String> = windows
        .keys()
        .filter(|id| id.starts_with("pwa_"))
        .cloned()
        .collect();
    Ok(CommandResponse::success(ids))
}

#[tauri::command]
pub fn update_pwa(
    app_id: String,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    let app_url: String = conn
        .query_row("SELECT url FROM apps WHERE id = ?", [app_id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("查询应用失败: {}", e))?;

    // 解析域名并删除对应的缓存目录
    if let Ok(url) = url::Url::parse(&app_url) {
        if let Some(domain) = url.host_str() {
            let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let cache_dir = app_data_dir.join("pwa_cache").join(domain);

            if cache_dir.exists() {
                log::info!(
                    "[PWA] Clearing cache for domain {}: {:?}",
                    domain,
                    cache_dir
                );
                std::fs::remove_dir_all(cache_dir).map_err(|e| format!("清理缓存失败: {}", e))?;
            }
        }
    }

    Ok(CommandResponse::success(true))
}

#[tauri::command]
pub fn close_pwa_window(
    app: AppHandle,
    _window_id: String,
) -> Result<CommandResponse<bool>, String> {
    #[cfg(not(mobile))]
    {
        if let Some(window) = app.get_webview_window(&_window_id) {
            window.destroy().ok();
            return Ok(CommandResponse::success(true));
        }
    }
    Ok(CommandResponse::success(true))
}

#[tauri::command]
pub fn kv_get_all(
    app_id: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<HashMap<String, String>>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM kv_store WHERE app_id = ?")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([app_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    for row in rows {
        if let Ok((k, v)) = row {
            map.insert(k, v);
        }
    }
    Ok(CommandResponse::success(map))
}

#[tauri::command]
pub fn kv_set(
    app_id: String,
    key: String,
    value: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("[KV] Set: app_id={}, key={}", app_id, key);
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO kv_store (app_id, key, value) VALUES (?, ?, ?)",
        [app_id, key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(true))
}

#[tauri::command]
pub fn kv_get(
    app_id: String,
    key: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<Option<String>>, String> {
    log::info!("[KV] Get: app_id={}, key={}", app_id, key);
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    let value = conn
        .query_row(
            "SELECT value FROM kv_store WHERE app_id = ? AND key = ?",
            [app_id, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(CommandResponse::success(value))
}

#[tauri::command]
pub fn kv_remove(
    app_id: String,
    key: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM kv_store WHERE app_id = ? AND key = ?",
        [app_id, key],
    )
    .map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(true))
}

#[tauri::command]
pub fn kv_clear(
    app_id: String,
    db: State<'_, DbConnection>,
) -> Result<CommandResponse<bool>, String> {
    let conn = db.inner().lock().map_err(|e| e.to_string())?;
    if app_id == "*" {
        // 清除所有 KV 数据
        conn.execute("DELETE FROM kv_store", [])
            .map_err(|e| e.to_string())?;
    } else {
        conn.execute("DELETE FROM kv_store WHERE app_id = ?", [app_id])
            .map_err(|e| e.to_string())?;
    }
    Ok(CommandResponse::success(true))
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

async fn fetch_manifest_info(
    url: &str,
) -> Result<ManifestInfo, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let html = client.get(url).send().await?.text().await?;

    // 1. 尝试从 HTML 中提取 Manifest 链接
    // <link rel="manifest" href="...">
    let manifest_url = regex::Regex::new(r#"<link\s+rel=["']manifest["']\s+href=["']([^"']+)["']"#)
        .unwrap()
        .captures(&html)
        .and_then(|cap| cap.get(1))
        .map(|m| absolute_url(m.as_str(), url));

    // 2. 尝试从 HTML 中提取标题 (作为备份)
    let html_title = regex::Regex::new(r#"<title>(.*?)</title>"#)
        .unwrap()
        .captures(&html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    // 3. 尝试从 HTML 中提取图标 (作为备份)
    let html_icon = regex::Regex::new(
        r#"<link\s+rel=["'](?:icon|shortcut icon|apple-touch-icon)["']\s+href=["']([^"']+)["']"#,
    )
    .unwrap()
    .captures(&html)
    .and_then(|cap| cap.get(1))
    .map(|m| absolute_url(m.as_str(), url));

    let mut info = ManifestInfo {
        name: html_title,
        icon_url: html_icon,
        manifest_url: manifest_url.clone(),
        start_url: Some(url.to_string()),
        scope: None,
        theme_color: None,
        background_color: None,
        display_mode: None,
    };

    // 4. 解析 Manifest JSON
    if let Some(m_url) = manifest_url {
        log::info!("Fetching manifest from: {}", m_url);
        if let Ok(resp) = client.get(&m_url).send().await {
            if let Ok(m) = resp.json::<serde_json::Value>().await {
                // 优先使用 Manifest 中的名称
                if let Some(name) = m["name"].as_str().or(m["short_name"].as_str()) {
                    info.name = Some(name.to_string());
                }

                // 解析图标：找最大的一个
                if let Some(icons) = m["icons"].as_array() {
                    let mut best_icon: Option<(i32, String)> = None;
                    for icon in icons {
                        if let Some(src) = icon["src"].as_str() {
                            let sizes = icon["sizes"].as_str().unwrap_or("0x0");
                            let width = sizes
                                .split('x')
                                .next()
                                .unwrap_or("0")
                                .parse::<i32>()
                                .unwrap_or(0);

                            if best_icon.is_none() || width > best_icon.as_ref().unwrap().0 {
                                best_icon = Some((width, absolute_url(src, &m_url)));
                            }
                        }
                    }
                    if let Some((_, src)) = best_icon {
                        info.icon_url = Some(src);
                    }
                }

                info.display_mode = m["display"].as_str().map(String::from);
                info.theme_color = m["theme_color"].as_str().map(String::from);
                info.background_color = m["background_color"].as_str().map(String::from);
                info.start_url = m["start_url"].as_str().map(|s| absolute_url(s, url));
            }
        }
    }

    Ok(info)
}

fn absolute_url(url: &str, base: &str) -> String {
    if url.starts_with("http") {
        return url.to_string();
    }
    if let Ok(base_url) = url::Url::parse(base) {
        if let Ok(joined) = base_url.join(url) {
            return joined.to_string();
        }
    }
    url.to_string()
}

fn save_app_to_db(conn: &Connection, app: &AppInfo) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO apps VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            &app.id,
            &app.name,
            &app.url,
            app.icon_url.as_ref().unwrap_or(&"".to_string()),
            app.manifest_url.as_ref().unwrap_or(&"".to_string()),
            &app.installed_at.to_string(),
            &app.updated_at.to_string(),
            app.start_url.as_ref().unwrap_or(&"".to_string()),
            app.scope.as_ref().unwrap_or(&"".to_string()),
            app.theme_color.as_ref().unwrap_or(&"".to_string()),
            app.background_color.as_ref().unwrap_or(&"".to_string()),
            &app.display_mode,
        ],
    )?;
    Ok(())
}

/// 获取配置项
#[tauri::command]
pub fn get_app_config(key: String) -> Result<CommandResponse<Option<String>>, String> {
    let conn = crate::DB_CONN.get()
        .ok_or("DB not initialized")?
        .lock()
        .map_err(|e| e.to_string())?;
    
    match crate::db::get_config(&conn, &key) {
        Ok(value) => Ok(CommandResponse::success(value)),
        Err(e) => Err(format!("Failed to get config: {}", e)),
    }
}

/// 设置配置项
#[tauri::command]
pub fn set_app_config(key: String, value: String) -> Result<CommandResponse<bool>, String> {
    let conn = crate::DB_CONN.get()
        .ok_or("DB not initialized")?
        .lock()
        .map_err(|e| e.to_string())?;
    
    match crate::db::set_config(&conn, &key, &value) {
        Ok(_) => Ok(CommandResponse::success(true)),
        Err(e) => Err(format!("Failed to set config: {}", e)),
    }
}
