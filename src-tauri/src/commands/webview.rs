use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use crate::models::CommandResponse;

/// 打开一个新的 WebView 窗口
#[tauri::command]
pub async fn open_webview(
    app: AppHandle,
    url: String,
    title: String,
    width: Option<f64>,
    height: Option<f64>,
    inject_adapt: Option<bool>,
) -> Result<CommandResponse<String>, String> {
    let label = format!("wv_{}", uuid::Uuid::new_v4().to_string().get(0..8).unwrap_or("tmp"));
    
    let mut builder = WebviewWindowBuilder::new(
        &app, 
        &label, 
        WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?)
    )
    .title(&title)
    .inner_size(width.unwrap_or(1000.0), height.unwrap_or(800.0));

    // 如果需要，注入 adapt.js
    if inject_adapt.unwrap_or(true) {
        // 这里的路径需要相对于 src-tauri/src/commands/webview.rs
        const ADAPT_JS: &str = include_str!("../../../adapt.js");
        builder = builder.initialization_script(ADAPT_JS);
    }

    builder.build().map_err(|e| e.to_string())?;
    
    log::info!("[WebView] Opened new window: {} (label: {})", url, label);
    Ok(CommandResponse::success(label))
}

/// 关闭当前的 WebView 窗口
#[tauri::command]
pub async fn close_current_webview(window: tauri::WebviewWindow) -> Result<CommandResponse<bool>, String> {
    window.close().map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(true))
}
