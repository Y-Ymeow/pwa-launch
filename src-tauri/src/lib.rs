pub mod commands;
pub mod db;
pub mod models;
pub mod utils;
pub mod local_server;

use std::sync::{Arc, Mutex, OnceLock};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_fs::init as fs_plugin;
use tauri_plugin_http::init as http_plugin;
use tauri_plugin_shell::init as shell_plugin;
use tokio::sync::RwLock;

// 全局数据库连接，用于在协议处理器中访问
pub static DB_CONN: OnceLock<Mutex<rusqlite::Connection>> = OnceLock::new();

pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_FORCE_SOFTWARE_RENDERING", "1");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }

    // Android 日志初始化
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(log::LevelFilter::Debug),
        );
    }

    let mut builder = tauri::Builder::default().plugin(tauri_plugin_fs::init());
    let host = Arc::new(Mutex::new(None::<String>));
    let host_clone = host.clone();

    // 非 Android 平台使用 tauri_plugin_log
    #[cfg(not(target_os = "android"))]
    {
        builder = builder.plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .build(),
        );
    }

    builder
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_audioplayer::init())
        .plugin(shell_plugin())
        .plugin(fs_plugin())
        .plugin(http_plugin())
        .register_uri_scheme_protocol("adapt", |_app, _request| {
            // 编译时嵌入 adapt.min.js 内容，避免运行时文件路径问题（Android 无法访问文件）
            const ADAPT_JS: &str = include_str!("../../adapt.min.js");

            log::info!(
                "[adapt] Serving adapt.min.js, size: {} bytes",
                ADAPT_JS.len()
            );

            http::Response::builder()
                .header("Content-Type", "application/javascript")
                .header("Cache-Control", "public, max-age=3600")
                .body(ADAPT_JS.as_bytes().to_vec())
                .expect("Failed to build response")
        })
        .register_uri_scheme_protocol("appdata", |_app, request| {
            use serde::{Deserialize, Serialize};

            #[derive(Deserialize, Serialize)]
            struct DataRequest {
                action: String,
                app_id: Option<String>,
                key: Option<String>,
                value: Option<String>,
                domain: Option<String>,
                cookies: Option<String>,
            }

            let body = request.body();
            let response = match serde_json::from_slice::<DataRequest>(body) {
                Ok(req) => {
                    // 使用全局 DB_CONN
                    match DB_CONN.get() {
                        Some(db_mutex) => {
                            let conn = db_mutex.lock().unwrap();
                            match req.action.as_str() {
                                "set" => {
                                    if let (Some(app_id), Some(key), Some(val)) = (req.app_id, req.key, req.value) {
                                        match conn.execute(
                                            "INSERT OR REPLACE INTO kv_store (app_id, key, value) VALUES (?1, ?2, ?3)",
                                            rusqlite::params![app_id, key, val],
                                        ) {
                                            Ok(_) => serde_json::json!({ "success": true, "action": "set" }),
                                            Err(e) => serde_json::json!({ "success": false, "error": format!("DB write error: {}", e) }),
                                        }
                                    } else {
                                        serde_json::json!({ "success": false, "error": "Missing parameters" })
                                    }
                                }
                                "get" => {
                                    if let (Some(app_id), Some(key)) = (req.app_id, req.key) {
                                        let result: Result<String, rusqlite::Error> = conn.query_row(
                                            "SELECT value FROM kv_store WHERE app_id = ?1 AND key = ?2",
                                            rusqlite::params![app_id, key],
                                            |row| row.get(0),
                                        );
                                        match result {
                                            Ok(val) => serde_json::json!({ "success": true, "action": "get", "data": val }),
                                            Err(_) => serde_json::json!({ "success": false, "action": "get", "data": null }),
                                        }
                                    } else {
                                        serde_json::json!({ "success": false, "error": "Missing parameters" })
                                    }
                                }
                                "cookie" => {
                                    // 直接保存 cookies 到数据库（使用外层的 conn）
                                    if let (Some(domain), Some(cookies)) = (req.domain, req.cookies) {
                                        match db::parse_and_save_cookie_string(&conn, "browser", &domain, &cookies) {
                                            Ok(_) => {
                                                log::debug!("[appdata] Cookies saved to DB for {}", domain);
                                                serde_json::json!({ "success": true, "action": "cookie" })
                                            }
                                            Err(e) => {
                                                log::error!("[appdata] Cookie save failed: {}", e);
                                                serde_json::json!({ "success": false, "error": format!("Cookie save error: {}", e) })
                                            }
                                        }
                                    } else {
                                        serde_json::json!({ "success": false, "error": "Missing domain or cookies" })
                                    }
                                }
                                _ => serde_json::json!({ "success": false, "error": "Unknown action" }),
                            }
                        }
                        None => serde_json::json!({ "success": false, "error": "DB not initialized" }),
                    }
                }
                Err(e) => serde_json::json!({ "success": false, "error": format!("Parse error: {}", e) }),
            };

            http::Response::builder()
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(response.to_string().into_bytes())
                .expect("Failed to build response")
        })
        .setup(move |app| {
            // 启动本地服务器（必须在 tokio 运行时中执行）
            tauri::async_runtime::block_on(async move {
                let app_data_dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&app_data_dir)?;
                db::init_db(&app_data_dir)?;

                let db_path = app_data_dir.join("pwa_container.db");

                // 只使用一个数据库连接，避免死锁
                let conn = rusqlite::Connection::open(&db_path)?;
                let _ = DB_CONN.set(Mutex::new(conn));

                let proxy_settings = Arc::new(RwLock::new(None::<commands::ProxySettings>));
                app.manage(proxy_settings.clone());

                // 启动本地 HTTP 服务器（Linux/Windows/macOS）
                local_server::start_local_server(proxy_settings).await;

                // 创建主窗口
                // dev 模式使用 Vite 端口（有热重载）
                #[cfg(dev)]
                let url = WebviewUrl::External(format!("http://localhost:1420").parse().unwrap());
                // release 模式使用打包后的前端文件
                #[cfg(not(dev))]
                let url = WebviewUrl::App(std::path::PathBuf::from("/"));

                // 从数据库读取 User-Agent
                let user_agent = if let Some(db_mutex) = DB_CONN.get() {
                    if let Ok(conn) = db_mutex.lock() {
                        crate::db::get_user_agent(&conn).unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // 从数据库读取代理设置
                let proxy_url: Option<reqwest::Url> = if let Some(db_mutex) = DB_CONN.get() {
                    if let Ok(conn) = db_mutex.lock() {
                        let enabled = crate::db::get_config(&conn, "proxy_enabled").ok().flatten() == Some("true".to_string());
                        if enabled {
                            let proxy_type = crate::db::get_config(&conn, "proxy_type").ok().flatten().unwrap_or_else(|| "http".to_string());
                            let proxy_host = crate::db::get_config(&conn, "proxy_host").ok().flatten().unwrap_or_default();
                            let proxy_port = crate::db::get_config(&conn, "proxy_port").ok().flatten().unwrap_or_else(|| "8080".to_string());
                            
                            if !proxy_host.is_empty() {
                                let port: u16 = proxy_port.parse().unwrap_or(8080);
                                let url_str = format!("{}://{}:{}", proxy_type, proxy_host, port);
                                url_str.parse::<reqwest::Url>().ok()
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let mut builder = WebviewWindowBuilder::new(app, "main", url)
                    .devtools(true)
                    .user_agent(&user_agent);

                // 设置代理（如果配置了）
                if let Some(proxy) = proxy_url {
                    log::info!("[WebView] Setting proxy: {}", proxy);
                    builder = builder.proxy_url(proxy);
                }

                let window = builder.build()?;

                // 设置窗口大小（仅在非 Android 平台）
                #[cfg(not(target_os = "android"))]
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: 1200.0,
                    height: 800.0,
                }));

                Ok(())
            })
        })
        .on_page_load(move |window, payload| {
            let mut host = host_clone.lock().unwrap();

            if host.is_none() {
                *host = Some(payload.url().to_string());
            }

            if let Some(url) = &*host {
                let _ = window.eval(&format!("window.__BASE_HOST__ = {:?};", url));
            }

            let url = payload.url().to_string();
            log::info!("Page loaded: {}", url);

            // 如果不是本地前端页面，注入浏览器 UI
            let is_local = url.contains("localhost") || url.contains("127.0.0.1") || url.starts_with("tauri://") || url.starts_with("http://localhost");
            if !is_local && !url.starts_with("about:blank") {
                log::info!("[Browser UI] Injecting to external page: {}", url);
                let _ = window.eval(commands::INJECT_BROWSER_UI);
                
                // 自动获取 cookies（包括 HttpOnly）并保存到数据库
                let window_clone = window.clone();
                std::thread::spawn(move || {
                    // 解析当前域名
                    let domain = if let Ok(parsed_url) = url::Url::parse(&url) {
                        parsed_url.host_str().map(|s| s.to_string())
                    } else {
                        None
                    };
                    
                    if let Some(domain) = domain {
                        match window_clone.cookies() {
                            Ok(cookies) => {
                                // 只过滤出当前域名的 cookies
                                let domain_cookies: Vec<_> = cookies.iter()
                                    .filter(|c| {
                                        // 检查 cookie 的 domain 是否匹配当前域名
                                        let cookie_domain = c.domain().unwrap_or("");
                                        cookie_domain == domain || 
                                        cookie_domain == format!(". {}", domain).trim_start() ||
                                        domain.ends_with(cookie_domain.strip_prefix('.').unwrap_or(cookie_domain))
                                    })
                                    .collect();
                                
                                if !domain_cookies.is_empty() {
                                    let cookie_str = domain_cookies.iter()
                                        .map(|c| format!("{}={}", c.name(), c.value()))
                                        .collect::<Vec<_>>()
                                        .join("; ");
                                    
                                    log::info!("[Auto Cookie] Got {} cookies for {}", domain_cookies.len(), domain);
                                    
                                    // 保存到数据库
                                    if let Some(db_mutex) = DB_CONN.get() {
                                        if let Ok(conn) = db_mutex.lock() {
                                            // 先清除该域名的旧 cookies
                                            if let Err(e) = conn.execute(
                                                "DELETE FROM cookies WHERE app_id = ?1 AND domain = ?2",
                                                rusqlite::params!["browser", &domain]
                                            ) {
                                                log::error!("[Auto Cookie] Failed to clear old cookies: {}", e);
                                            }
                                            // 保存新 cookies
                                            if let Err(e) = db::parse_and_save_cookie_string(&conn, "browser", &domain, &cookie_str) {
                                                log::error!("[Auto Cookie] Failed to save: {}", e);
                                            } else {
                                                log::info!("[Auto Cookie] Saved {} cookies for {}", domain_cookies.len(), domain);
                                            }
                                        }
                                    }
                                } else {
                                    log::debug!("[Auto Cookie] No cookies for domain: {}", domain);
                                }
                            }
                            Err(e) => {
                                log::error!("[Auto Cookie] Failed to get cookies: {}", e);
                            }
                        }
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::install_pwa,
            commands::uninstall_pwa,
            commands::list_apps,
            commands::launch_app,
            commands::get_app_info,
            commands::list_running_pwas,
            commands::update_pwa,
            commands::close_pwa_window,
            commands::clear_data,
            commands::backup_data,
            commands::restore_data,
            commands::proxy_fetch,
            commands::get_cookies,
            commands::set_cookies,
            commands::clear_cookies,
            commands::get_cookie_domains,
            commands::get_all_cookies,
            commands::set_proxy,
            commands::get_proxy,
            commands::disable_proxy,
            commands::opfs_write_file,
            commands::opfs_read_file,
            commands::opfs_delete_file,
            commands::opfs_list_dir,
            commands::open_file_dialog,
            commands::read_file_content,
            commands::read_file_range,
            commands::resolve_local_file_url,
            commands::sync_webview_cookies,
            commands::get_proxy_cookies,
            commands::fs_read_dir,
            commands::fs_write_file,
            commands::fs_create_dir,
            commands::fs_remove,
            commands::fs_exists,
            commands::check_storage_permission,
            commands::request_storage_permission,
            commands::kv_set,
            commands::kv_get,
            commands::kv_get_all,
            commands::kv_remove,
            commands::set_keep_screen_on,
            commands::get_keep_screen_on,
            commands::kv_clear,
            commands::navigate_to_url,
            commands::navigate_back,
            commands::get_webview_info,
            commands::reinject_browser_ui,
            commands::check_browser_ui,
            commands::eval_js,
            commands::get_app_config,
            commands::set_app_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    run();
}
