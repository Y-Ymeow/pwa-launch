pub mod commands;
pub mod db;
pub mod models;
pub mod utils;

use tauri::Manager;
use tauri_plugin_shell::init as shell_plugin;
use tauri_plugin_fs::init as fs_plugin;
use tauri_plugin_http::init as http_plugin;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub fn run() {
    // 禁用 GPU/硬件加速以兼容更多设备
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("WEBKIT_FORCE_SOFTWARE_RENDERING", "1");
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("GDK_BACKEND", "x11");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    #[cfg(target_os = "windows")]
    {
        std::env::set_var("WEBVIEW2_SOFTWARE_RENDERER", "1");
    }
    #[cfg(target_os = "macos")]
    {
        std::env::set_var("WEBKIT_DISABLE_WEB_PROCESS_SIDE_DISPLAY", "1");
    }

    env_logger::init();

    tauri::Builder::default()
        .plugin(shell_plugin())
        .plugin(fs_plugin())
        .plugin(http_plugin())
        .setup(|app| {
            // 初始化应用数据目录
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            // 初始化数据库
            db::init_db(&app_data_dir)?;

            // 注册数据库状态
            let db_path = app_data_dir.join("pwa_container.db");
            let conn = rusqlite::Connection::open(&db_path)?;
            app.manage(std::sync::Mutex::new(conn));

            // 注册 Cookie 存储 - 全局共享
            let cookie_store: commands::CookieStore = Arc::new(RwLock::new(HashMap::new()));
            app.manage(cookie_store);

            // 注册代理配置
            let proxy_config: commands::ProxyConfig = Arc::new(RwLock::new(None));
            app.manage(proxy_config);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::install_pwa,
            commands::uninstall_pwa,
            commands::list_apps,
            commands::launch_app,
            commands::close_pwa_window,
            commands::list_running_pwas,
            commands::clear_data,
            commands::backup_data,
            commands::restore_data,
            commands::create_shortcut,
            commands::get_app_info,
            commands::update_pwa,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// 移动端入口
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    run();
}
