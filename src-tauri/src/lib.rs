pub mod commands;
pub mod db;
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
        .plugin(tauri_plugin_os::init())
        .plugin(shell_plugin())
        .plugin(fs_plugin())
        .plugin(http_plugin())
        .register_uri_scheme_protocol("adapt", |_app, _request| {
            // 返回 adapt.js 桥接脚本
            let script = r#"(function(){if(window.__TAURI_ADAPT_INJECTED__)return;window.__TAURI_ADAPT_INJECTED__=!0,console.log("[Tauri Adapt] Bridge loaded");const generateId=()=>Date.now().toString(36)+Math.random().toString(36).substr(2),waitForParent=()=>new Promise(e=>{if(window.parent!==window){window.parent.postMessage({type:"ADAPT_READY"},"*");const t=n=>{n.data?.type==="ADAPT_PARENT_READY"&&(window.removeEventListener("message",t),e(!0))};window.addEventListener("message",t),setTimeout(()=>{window.removeEventListener("message",t),e(!1)},1e3)}else e(!1)}),tauriBridge={_ready:!1,_pending:new Map,async init(){this._ready=await waitForParent(),console.log("[Tauri Adapt] Parent ready:",this._ready)},async invoke(e,t={}){if(!this._ready)throw new Error("Tauri Adapt not ready");return new Promise((n,r)=>{const i=generateId(),o=setTimeout(()=>{this._pending.delete(i),r(new Error("Invoke timeout"))},3e4);this._pending.set(i,{resolve:n,reject:r,timeout:o}),window.parent.postMessage({type:"ADAPT_INVOKE",id:i,cmd:e,payload:t},"*")})},_handleResponse(e){const{id:t,result:n,error:r}=e,i=this._pending.get(t);i&&(clearTimeout(i.timeout),this._pending.delete(t),r?i.reject(new Error(r)):i.resolve(n))},async fetch(e,t={}){const n=e.toString();if(n.startsWith("tauri://")){const e=n.match(/tauri:\/\/(.+)/);if(e)return this.invoke(e[1],t).then(e=>new Response(JSON.stringify(e),{status:200,headers:{"Content-Type":"application/json"}}))}try{const e=new URL(n,window.location.href);if(e.origin!==window.location.origin){const e=await this.invoke("proxy_fetch",{url:n,method:t.method||"GET",headers:t.headers,body:t.body});return new Response(e.body,{status:e.status,headers:e.headers})}}catch(e){}return fetch(e,t)}};window.addEventListener("message",e=>{e.data?.type==="ADAPT_RESPONSE"&&tauriBridge._handleResponse(e.data)}),tauriBridge.init(),window.__TAURI__=tauriBridge,window.tauri=tauriBridge;const originalFetch=window.fetch;window.fetch=function(...e){return tauriBridge.fetch.apply(tauriBridge,e)},window.dispatchEvent(new CustomEvent("tauri-ready"))})();"#;
            
            http::Response::builder()
                .header("Content-Type", "application/javascript")
                .header("Cache-Control", "public, max-age=3600")
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "GET, OPTIONS")
                .header("Access-Control-Allow-Headers", "Content-Type")
                .body(script.as_bytes().to_vec())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    run();
}
