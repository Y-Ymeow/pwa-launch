use tauri::{AppHandle, Manager, WebviewWindow};
use crate::models::CommandResponse;

/// 在当前 WebView 中打开 URL（不创建新窗口）
/// 由前端 adapt.js 控制导航和显示浏览器 UI
#[tauri::command]
pub async fn navigate_to_url(
    window: WebviewWindow,
    url: String,
) -> Result<CommandResponse<bool>, String> {
    // 使用 eval 通知前端导航
    let script = format!(
        r#"window.__TAURI_NAVIGATE__ && window.__TAURI_NAVIGATE__("{}");"#,
        url.replace("\"", "\\\"")
    );
    
    window.eval(&script)
        .map_err(|e| format!("导航失败: {:?}", e))?;
    
    log::info!("[WebView] Navigate to: {}", url);
    Ok(CommandResponse::success(true))
}

/// 返回上一页
#[tauri::command]
pub async fn navigate_back(
    window: WebviewWindow,
) -> Result<CommandResponse<bool>, String> {
    let script = r#"window.__TAURI_GO_BACK__ && window.__TAURI_GO_BACK__();"#;
    
    window.eval(&script)
        .map_err(|e| format!("返回失败: {:?}", e))?;
    
    log::info!("[WebView] Navigate back");
    Ok(CommandResponse::success(true))
}

/// 获取当前窗口的 WebView 列表（用于移动端获取 webview 引用）
#[tauri::command]
pub async fn get_webview_info(
    app: AppHandle,
) -> Result<CommandResponse<serde_json::Value>, String> {
    let windows: Vec<String> = app.webview_windows()
        .keys()
        .cloned()
        .collect();
    
    Ok(CommandResponse::success(serde_json::json!({
        "windows": windows,
    })))
}
