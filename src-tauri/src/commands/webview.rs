use tauri::{AppHandle, Manager};
use crate::models::CommandResponse;

/// 打开一个新的 WebView 窗口（桌面端）或在当前 WebView 导航（移动端）
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
    
    #[cfg(not(mobile))]
    {
        // 桌面端：创建新窗口
        let window = tauri::window::WindowBuilder::new(&app, &label)
            .title(&title)
            .inner_size(width.unwrap_or(1000.0), height.unwrap_or(800.0))
            .build()
            .map_err(|e| format!("创建窗口失败: {:?}", e))?;

        let mut webview_builder = tauri::webview::WebviewBuilder::new(
            format!("{}_webview", label),
            tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?),
        );

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
    }
    
    #[cfg(mobile)]
    {
        // 移动端：在当前窗口创建新 WebView 或导航
        // 找到当前活动的 WebView 窗口
        let windows = app.webview_windows();
        if let Some((_, window)) = windows.iter().next() {
            // 在当前窗口添加新的 WebView
            let webview_label = format!("{}_webview", label);
            let mut webview_builder = tauri::webview::WebviewBuilder::new(
                &webview_label,
                tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?),
            );

            if inject_adapt.unwrap_or(true) {
                const ADAPT_JS: &str = include_str!("../../../adapt.js");
                webview_builder = webview_builder.initialization_script(ADAPT_JS);
            }

            // 移动端全屏显示
            let size = window.inner_size().map_err(|e| format!("获取窗口大小失败: {:?}", e))?;
            window.add_child(
                webview_builder,
                tauri::LogicalPosition::new(0.0, 0.0),
                size,
            ).map_err(|e| format!("添加 WebView 失败: {:?}", e))?;
        } else {
            return Err("No active window found".to_string());
        }
    }
    
    log::info!("[WebView] Opened new window: {} (label: {})", url, label);
    Ok(CommandResponse::success(label))
}

/// 关闭当前的 WebView 窗口
#[tauri::command]
pub async fn close_current_webview(
    window: tauri::WebviewWindow,
    label: Option<String>,
) -> Result<CommandResponse<bool>, String> {
    #[cfg(not(mobile))]
    {
        // 桌面端：关闭窗口
        let _ = window.close();
    }
    
    #[cfg(mobile)]
    {
        // 移动端：移除指定的 WebView
        if let Some(webview_label) = label {
            window.remove_child(&webview_label).ok();
        }
    }
    
    Ok(CommandResponse::success(true))
}
