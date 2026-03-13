use tauri::{AppHandle, Manager};
use crate::models::CommandResponse;

/// 浏览器 UI 脚本（注入到打开的页面，显示地址栏和返回按钮）
const BROWSER_UI_JS: &str = r#"
(function() {
    if (window.__BROWSER_UI_INJECTED__) return;
    window.__BROWSER_UI_INJECTED__ = true;
    
    // 创建浏览器 UI
    const ui = document.createElement('div');
    ui.id = 'pwa-browser-ui';
    ui.innerHTML = `
        <div style="
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            height: 48px;
            background: #1a1a2e;
            display: flex;
            align-items: center;
            padding: 0 12px;
            z-index: 2147483647;
            box-shadow: 0 2px 8px rgba(0,0,0,0.3);
        ">
            <button id="pwa-back-btn" style="
                background: rgba(255,255,255,0.1);
                border: none;
                color: white;
                padding: 8px 16px;
                border-radius: 4px;
                cursor: pointer;
                font-size: 14px;
                margin-right: 12px;
            ">← 返回</button>
            <div style="
                flex: 1;
                background: rgba(255,255,255,0.1);
                border-radius: 4px;
                padding: 8px 12px;
                color: rgba(255,255,255,0.8);
                font-size: 13px;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            ">${location.href}</div>
        </div>
        <div style="height: 48px;"></div>
    `;
    
    document.body.insertBefore(ui, document.body.firstChild);
    
    // 返回按钮点击事件
    document.getElementById('pwa-back-btn').onclick = function() {
        if (window.__TAURI__) {
            window.__TAURI__.invoke('close_current_webview', {});
        } else {
            history.back();
        }
    };
})();
"#;

/// 打开一个新的 WebView 窗口（桌面端）或在当前 WebView 导航（移动端）
#[tauri::command]
pub async fn open_webview(
    app: AppHandle,
    url: String,
    title: String,
    width: Option<f64>,
    height: Option<f64>,
    _inject_adapt: Option<bool>,
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

        let webview_builder = tauri::webview::WebviewBuilder::new(
            format!("{}_webview", label),
            tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?),
        )
        .initialization_script(BROWSER_UI_JS);

        window.add_child(
            webview_builder,
            tauri::LogicalPosition::new(0.0, 0.0),
            window.inner_size().unwrap(),
        ).map_err(|e| format!("添加 WebView 失败: {:?}", e))?;
    }
    
    #[cfg(mobile)]
    {
        // 移动端：使用 tauri::Webview 创建
        if let Some(window) = app.get_webview_window("main") {
            let webview_label = format!("{}_webview", label);
            let webview_builder = tauri::webview::WebviewBuilder::new(
                &webview_label,
                tauri::WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?),
            )
            .initialization_script(BROWSER_UI_JS);
            
            let size: tauri::PhysicalSize<u32> = window.inner_size().map_err(|e: tauri::Error| format!("获取窗口大小失败: {:?}", e))?;
            window.add_child(
                webview_builder,
                tauri::LogicalPosition::new(0.0, 0.0),
                size,
            ).map_err(|e: tauri::Error| format!("添加 WebView 失败: {:?}", e))?;
        }
    }
    
    log::info!("[WebView] Opened: {} (label: {})", url, label);
    Ok(CommandResponse::success(label))
}

/// 关闭当前的 WebView
#[tauri::command]
pub async fn close_current_webview(app: AppHandle) -> Result<CommandResponse<bool>, String> {
    // 找到并关闭 webview 窗口
    let windows: std::collections::HashMap<String, tauri::WebviewWindow> = app.webview_windows();
    for (label, window) in windows {
        if label.starts_with("wv_") {
            let _ = window.close();
            return Ok(CommandResponse::success(true));
        }
    }
    
    Ok(CommandResponse::success(false))
}
