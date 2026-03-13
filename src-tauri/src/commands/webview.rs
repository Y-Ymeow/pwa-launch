use tauri::AppHandle;
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
    
    // 先创建窗口
    let window = tauri::window::WindowBuilder::new(&app, &label)
        .title(&title)
        .inner_size(width.unwrap_or(1000.0), height.unwrap_or(800.0))
        .build()
        .map_err(|e| format!("创建窗口失败: {:?}", e))?;

    // 创建 WebView
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        format!("{}_webview", label),
        tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?),
    );

    // 如果需要，注入 adapt.js
    if inject_adapt.unwrap_or(true) {
        const ADAPT_JS: &str = include_str!("../../../adapt.js");
        webview_builder = webview_builder.initialization_script(ADAPT_JS);
    }

    let size = window.inner_size().map_err(|e| format!("获取窗口大小失败: {:?}", e))?;
    window.add_child(
        webview_builder,
        tauri::LogicalPosition::new(0.0, 0.0),
        size,
    ).map_err(|e| format!("添加 WebView 失败: {:?}", e))?;
    
    log::info!("[WebView] Opened new window: {} (label: {})", url, label);
    Ok(CommandResponse::success(label))
}

/// 关闭当前的 WebView 窗口
#[tauri::command]
pub async fn close_current_webview(window: tauri::WebviewWindow) -> Result<CommandResponse<bool>, String> {
    // 尝试关闭窗口
    let _ = window.close();
    Ok(CommandResponse::success(true))
}
