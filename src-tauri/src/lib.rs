pub mod commands;
pub mod db;
pub mod models;
pub mod utils;
pub mod local_server;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_fs::init as fs_plugin;
use tauri_plugin_http::init as http_plugin;
use tauri_plugin_shell::init as shell_plugin;
use tokio::sync::RwLock;

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
        .plugin(shell_plugin())
        .plugin(fs_plugin())
        .plugin(http_plugin())
        .register_uri_scheme_protocol("static", |_app, request| {
            commands::static_protocol::handle_static_request(request)
        })
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
        .register_uri_scheme_protocol("fetch", |app, request| {
            // Fetch 协议：fetch://example.com/path -> 代理 HTTP 请求
            // 比 invoke 快，不需要 postMessage 桥接
            // 从 app state 获取 CookieStore
            let cookie_store = app.app_handle().state::<commands::CookieStore>();
            match commands::fetch_protocol::handle_fetch_request(&request, Some(&cookie_store)) {
                Ok(res) => res,
                Err(e) => {
                    log::error!("[FetchProtocol] Error: {}", e);
                    http::Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(e.to_string().into_bytes())
                        .unwrap()
                }
            }
        })
        .setup(move |app| {
            // 启动本地服务器（必须在 tokio 运行时中执行）
            tauri::async_runtime::block_on(async move {
                let app_data_dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&app_data_dir)?;
                db::init_db(&app_data_dir)?;

                let db_path = app_data_dir.join("pwa_container.db");
                let conn = rusqlite::Connection::open(&db_path)?;
                app.manage(std::sync::Mutex::new(conn));

                // 初始化全局状态
                let cookie_store = Arc::new(RwLock::new(HashMap::<
                    String,
                    HashMap<String, HashMap<String, String>>,
                >::new()));
                app.manage(cookie_store.clone());

                let proxy_settings = Arc::new(RwLock::new(None::<commands::ProxySettings>));
                app.manage(proxy_settings.clone());

                // 启动本地 HTTP 服务器（Linux/Windows/macOS）
                #[cfg(not(target_os = "android"))]
                {
                    local_server::start_local_server(cookie_store, proxy_settings).await;
                }

                // 创建主窗口
                // dev 模式使用 Vite 端口（有热重载）
                #[cfg(dev)]
                let url = WebviewUrl::External("http://localhost:1420".parse().unwrap());
                // release 模式使用打包后的前端文件
                #[cfg(not(dev))]
                let url = WebviewUrl::App(std::path::PathBuf::from("/"));

                WebviewWindowBuilder::new(app, "main", url)
                    .title("PWA Container")
                    .inner_size(1200.0, 800.0)
                    .center()
                    .devtools(true)
                    .build()?;

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
            commands::kv_clear,
            commands::navigate_to_url,
            commands::navigate_back,
            commands::get_webview_info,
            commands::reinject_browser_ui,
            commands::check_browser_ui,
            commands::eval_js,
            commands::audio_play,
            commands::audio_pause,
            commands::audio_resume,
            commands::audio_stop,
            commands::audio_set_volume,
            commands::audio_get_state,
            commands::audio_get_position,
            commands::audio_get_duration,
            commands::audio_seek,
            commands::audio_get_current_url,
            commands::audio_set_loop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    run();
}