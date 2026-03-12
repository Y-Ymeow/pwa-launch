pub mod commands;
pub mod db;
pub mod local_server;
pub mod models;
pub mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, Manager, Runtime};
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

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                    file_name: Some("pwa_container".to_string()),
                }))
                .build(),
        )
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(shell_plugin())
        .plugin(fs_plugin())
        .plugin(http_plugin())
        .register_uri_scheme_protocol("static", |_app, request| {
            commands::static_protocol::handle_static_request(request)
        })
        .register_asynchronous_uri_scheme_protocol("stream", move |_app, request, responder| {
            match commands::stream_file_protocol::handle_stream_request(request) {
                Ok(http_response) => responder.respond(http_response),
                Err(e) => responder.respond(
                    http::Response::builder()
                        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
                        .header("Content-Type", "text/plain")
                        .body(e.to_string().into_bytes())
                        .unwrap(),
                ),
            }
        })
        .register_uri_scheme_protocol("adapt", |_app, _request| {
            // 编译时嵌入 adapt.js 内容，避免运行时文件路径问题（Android 无法访问文件）
            const ADAPT_JS: &str = include_str!("../../adapt.js");
            
            log::info!("[adapt] Serving adapt.js, size: {} bytes", ADAPT_JS.len());
            
            http::Response::builder()
                .header("Content-Type", "application/javascript")
                .header("Cache-Control", "public, max-age=3600")
                .body(ADAPT_JS.as_bytes().to_vec())
                .expect("Failed to build response")
        })
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            db::init_db(&app_data_dir)?;

            let db_path = app_data_dir.join("pwa_container.db");
            let conn = rusqlite::Connection::open(&db_path)?;
            app.manage(std::sync::Mutex::new(conn));

            // 初始化全局状态
            app.manage(Arc::new(RwLock::new(HashMap::<
                String,
                HashMap<String, HashMap<String, String>>,
            >::new()))); // CookieStore
            
            // 启动本地文件服务器
            match local_server::init_local_server(8765) {
                Ok(port) => log::info!("[LocalServer] Started on port {}", port),
                Err(e) => log::error!("[LocalServer] Failed to start: {}", e),
            }
            app.manage(Arc::new(RwLock::new(None::<commands::ProxySettings>))); // ProxyConfig

            Ok(())
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
            commands::resolve_local_file_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    run();
}
