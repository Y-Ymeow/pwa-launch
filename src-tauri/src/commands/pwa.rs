use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};

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
        .map_err(|e| e.to_string())?;
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
        let window = tauri::window::WindowBuilder::new(&app, &window_id)
            .title(&_app_name)
            .inner_size(1200.0, 800.0)
            .build()
            .map_err(|e| e.to_string())?;
        let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let user_data_dir = get_app_data_dir(&app_id, &app_data_dir);
        let webview_builder = tauri::webview::WebviewBuilder::new(
            format!("{}_webview", window_id),
            tauri::WebviewUrl::External(
                app_url
                    .parse()
                    .map_err(|e: url::ParseError| e.to_string())?,
            ),
        )
        .data_directory(user_data_dir);
        window
            .add_child(
                webview_builder,
                tauri::LogicalPosition::new(0, 0),
                window.inner_size().unwrap(),
            )
            .map_err(|e| e.to_string())?;
    }

    #[cfg(mobile)]
    {
        // 移动端通过插件调用 Android 的 launchPwaAsNewTask 方法
        // 由于不能直接调用插件，这里发送自定义事件给前端，让前端通过插件启动
        app.emit::<serde_json::Value>("launch-pwa-request", serde_json::json!({
            "appId": app_id,
            "name": _app_name,
            "url": app_url,
            "displayMode": _display_mode
        })).map_err(|e: tauri::Error| e.to_string())?;
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
pub fn update_pwa(_app_id: String) -> Result<CommandResponse<bool>, String> {
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
    #[cfg(mobile)]
    {
        if let Some(main_window) = app.get_webview_window("main") {
            let main_url = url::Url::parse("tauri://localhost/index.html").unwrap();
            let _ = main_window.navigate(main_url);
        }
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
        .user_agent("Mozilla/5.0")
        .build()?;
    let html = client.get(url).send().await?.text().await?;
    let manifest_url = regex::Regex::new(r#"href=["']([^"']+\.json|[^"']+\.webmanifest)["']"#)
        .unwrap()
        .captures(&html)
        .and_then(|cap| cap.get(1))
        .map(|m| absolute_url(m.as_str(), url));
    let mut info = ManifestInfo {
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
        if let Ok(m) = client
            .get(&m_url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await
        {
            info.name = m["name"].as_str().map(String::from);
            info.display_mode = m["display"].as_str().map(String::from);
        }
    }
    Ok(info)
}
fn absolute_url(url: &str, base: &str) -> String {
    if url.starts_with("http") {
        return url.to_string();
    }
    url::Url::parse(base)
        .ok()
        .and_then(|b| b.join(url).ok())
        .map(|u| u.to_string())
        .unwrap_or_else(|| url.to_string())
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
