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
            // 从文件读取 adapt.js
            let mut possible_paths: Vec<std::path::PathBuf> = vec![];
            
            // 1. 尝试当前工作目录的 adapt.js
            possible_paths.push(std::path::PathBuf::from("adapt.js"));
            
            // 2. 尝试从 exe 路径推导
            if let Ok(exe) = std::env::current_exe() {
                log::info!("[adapt] Current exe: {:?}", exe);
                if let Some(exe_dir) = exe.parent() {
                    // exe_dir = target/debug/
                    possible_paths.push(exe_dir.join("resources/adapt.js"));
                    
                    if let Some(target_dir) = exe_dir.parent() {
                        // target_dir = target/
                        possible_paths.push(target_dir.join("resources/adapt.js"));
                        possible_paths.push(target_dir.join("adapt.js"));
                        
                        if let Some(project_root) = target_dir.parent() {
                            // project_root = 项目根目录
                            possible_paths.push(project_root.join("adapt.js"));
                            possible_paths.push(project_root.join("src-tauri").join("adapt.js"));
                        }
                    }
                }
            }
            
            // 3. 尝试使用 manifest_dir (编译时确定)
            if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                let manifest_path = std::path::PathBuf::from(&manifest_dir);
                possible_paths.push(manifest_path.join("adapt.js"));
                // 项目根目录是 src-tauri 的父目录
                if let Some(project_root) = manifest_path.parent() {
                    possible_paths.push(project_root.join("adapt.js"));
                }
            }
            
            log::info!("[adapt] Searching {} paths for adapt.js", possible_paths.len());
            
            let script = possible_paths.iter()
                .find(|p| {
                    let exists = p.exists();
                    log::info!("[adapt] Checking {:?}: {}", p, exists);
                    exists
                })
                .and_then(|p| {
                    let content = std::fs::read_to_string(p).ok();
                    log::info!("[adapt] Found at {:?}, content size: {:?}", p, content.as_ref().map(|c| c.len()));
                    content
                })
                .unwrap_or_else(|| {
                    log::error!("[adapt] adapt.js not found in any location!");
                    "console.error('adapt.js not found');".to_string()
                });
            
            http::Response::builder()
                .header("Content-Type", "application/javascript")
                .header("Cache-Control", "public, max-age=3600")
                .body(script.into_bytes())
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
