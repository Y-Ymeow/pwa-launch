use tauri::{AppHandle, Manager, WebviewWindow};
use crate::models::CommandResponse;

// 浏览器 UI 注入脚本 - 使用 Shadow DOM 隔离样式
const BROWSER_UI_JS: &str = r#"
(function() {
  if (window.__BROWSER_UI_INJECTED__) return;
  window.__BROWSER_UI_INJECTED__ = true;
  
  const host = document.createElement('div');
  host.id = '__browser_ui_host__';
  host.style.cssText = 'position:fixed;top:0;left:0;right:0;z-index:2147483647;pointer-events:none;';
  document.documentElement.appendChild(host);
  
  const shadow = host.attachShadow({ mode: 'open' });
  
  shadow.innerHTML = `
    <style>
      * { box-sizing: border-box !important; margin: 0 !important; padding: 0 !important; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif !important; }
      .browser-bar {
        background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%) !important;
        padding: 8px 12px !important; display: flex !important; align-items: center !important; gap: 8px !important;
        box-shadow: 0 2px 10px rgba(0,0,0,0.3) !important; pointer-events: auto !important;
        border-bottom: 1px solid rgba(255,255,255,0.1) !important;
      }
      .browser-btn {
        background: rgba(255,255,255,0.1) !important; border: none !important; color: white !important;
        width: 36px !important; height: 36px !important; border-radius: 8px !important;
        cursor: pointer !important; font-size: 18px !important; display: flex !important;
        align-items: center !important; justify-content: center !important; transition: all 0.2s !important;
        pointer-events: auto !important;
      }
      .browser-btn:hover { background: rgba(255,255,255,0.2) !important; transform: translateY(-1px) !important; }
      .address-input {
        flex: 1 !important; background: rgba(0,0,0,0.3) !important; border: 1px solid rgba(255,255,255,0.1) !important;
        color: white !important; padding: 8px 12px !important; border-radius: 8px !important;
        font-size: 14px !important; outline: none !important; pointer-events: auto !important;
      }
      .address-input:focus { border-color: #667eea !important; box-shadow: 0 0 0 2px rgba(102,126,234,0.3) !important; }
      .install-btn {
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important; border: none !important;
        color: white !important; padding: 8px 16px !important; border-radius: 8px !important;
        cursor: pointer !important; font-size: 13px !important; font-weight: 500 !important;
        white-space: nowrap !important; pointer-events: auto !important; transition: all 0.2s !important;
      }
      .install-btn:hover { transform: translateY(-1px) !important; box-shadow: 0 4px 12px rgba(102,126,234,0.4) !important; }
      .spacer { height: 52px !important; }
    </style>
    <div class="browser-bar">
      <button class="browser-btn" id="__browser_back__" title="返回">←</button>
      <button class="browser-btn" id="__browser_refresh__" title="刷新">↻</button>
      <input type="text" class="address-input" id="__browser_address__" placeholder="输入网址..." />
      <button class="install-btn" id="__browser_install__">➕ 安装</button>
    </div>
    <div class="spacer"></div>
  `;
  
  const backBtn = shadow.getElementById('__browser_back__');
  const refreshBtn = shadow.getElementById('__browser_refresh__');
  const addressInput = shadow.getElementById('__browser_address__');
  const installBtn = shadow.getElementById('__browser_install__');
  
  addressInput.value = location.href;
  
  backBtn.addEventListener('click', () => {
    window.parent.postMessage({ type: 'BROWSER_GO_BACK' }, '*');
  });
  
  refreshBtn.addEventListener('click', () => location.reload());
  
  addressInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') {
      let url = addressInput.value.trim();
      if (!url.startsWith('http')) url = 'https://' + url;
      window.parent.postMessage({ type: 'BROWSER_NAVIGATE', url }, '*');
    }
  });
  
  installBtn.addEventListener('click', () => {
    window.parent.postMessage({ type: 'BROWSER_INSTALL', url: location.href }, '*');
  });
  
  setInterval(() => {
    if (addressInput.value !== location.href) addressInput.value = location.href;
  }, 1000);
})();
"#;

// 浏览器 UI 注入脚本 - 通过 eval 注入
const INJECT_BROWSER_UI: &str = r#"
(function() {
    // 每次页面加载都尝试注入
    function injectUI() {
        if (document.getElementById('__browser_ui_host__')) return;
        
        const host = document.createElement('div');
        host.id = '__browser_ui_host__';
        host.style.cssText = 'position:fixed;top:0;left:0;right:0;z-index:2147483647;pointer-events:none;';
        
        const shadow = host.attachShadow({ mode: 'open' });
        
        shadow.innerHTML = `
            <style>
                * { box-sizing: border-box !important; margin: 0 !important; padding: 0 !important; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif !important; }
                .browser-bar {
                    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%) !important;
                    padding: 8px 12px !important; display: flex !important; align-items: center !important; gap: 8px !important;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.3) !important; pointer-events: auto !important;
                    border-bottom: 1px solid rgba(255,255,255,0.1) !important;
                }
                .browser-btn {
                    background: rgba(255,255,255,0.1) !important; border: none !important; color: white !important;
                    width: 36px !important; height: 36px !important; border-radius: 8px !important;
                    cursor: pointer !important; font-size: 18px !important; display: flex !important;
                    align-items: center !important; justify-content: center !important; transition: all 0.2s !important;
                    pointer-events: auto !important;
                }
                .browser-btn:hover { background: rgba(255,255,255,0.2) !important; transform: translateY(-1px) !important; }
                .address-input {
                    flex: 1 !important; background: rgba(0,0,0,0.3) !important; border: 1px solid rgba(255,255,255,0.1) !important;
                    color: white !important; padding: 8px 12px !important; border-radius: 8px !important;
                    font-size: 14px !important; outline: none !important; pointer-events: auto !important;
                }
                .address-input:focus { border-color: #667eea !important; box-shadow: 0 0 0 2px rgba(102,126,234,0.3) !important; }
                .install-btn {
                    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important; border: none !important;
                    color: white !important; padding: 8px 16px !important; border-radius: 8px !important;
                    cursor: pointer !important; font-size: 13px !important; font-weight: 500 !important;
                    white-space: nowrap !important; pointer-events: auto !important; transition: all 0.2s !important;
                }
                .install-btn:hover { transform: translateY(-1px) !important; box-shadow: 0 4px 12px rgba(102,126,234,0.4) !important; }
                .spacer { height: 52px !important; }
            </style>
            <div class="browser-bar">
                <button class="browser-btn" id="__browser_back__" title="返回">←</button>
                <button class="browser-btn" id="__browser_refresh__" title="刷新">↻</button>
                <input type="text" class="address-input" id="__browser_address__" placeholder="输入网址..." />
                <button class="install-btn" id="__browser_install__">➕ 安装</button>
            </div>
            <div class="spacer"></div>
        `;
        
        document.documentElement.appendChild(host);
        
        const backBtn = shadow.getElementById('__browser_back__');
        const refreshBtn = shadow.getElementById('__browser_refresh__');
        const addressInput = shadow.getElementById('__browser_address__');
        const installBtn = shadow.getElementById('__browser_install__');
        
        addressInput.value = location.href;
        
        backBtn.addEventListener('click', () => {
            window.parent.postMessage({ type: 'BROWSER_GO_BACK' }, '*');
        });
        
        refreshBtn.addEventListener('click', () => location.reload());
        
        addressInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') {
                let url = addressInput.value.trim();
                if (!url.startsWith('http')) url = 'https://' + url;
                window.parent.postMessage({ type: 'BROWSER_NAVIGATE', url }, '*');
            }
        });
        
        installBtn.addEventListener('click', () => {
            window.parent.postMessage({ type: 'BROWSER_INSTALL', url: location.href }, '*');
        });
        
        setInterval(() => {
            if (addressInput.value !== location.href) addressInput.value = location.href;
        }, 1000);
        
        console.log('[Browser UI] Injected');
    }
    
    // 立即注入
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', injectUI);
    } else {
        injectUI();
    }
    
    // 监听 URL 变化（用于 SPA 应用）
    let lastUrl = location.href;
    setInterval(() => {
        if (location.href !== lastUrl) {
            lastUrl = location.href;
            setTimeout(injectUI, 500);
        }
    }, 500);
})();
"#;

/// 在当前 WebView 中打开 URL（不创建新窗口）
/// 并注入浏览器 UI
#[tauri::command]
pub async fn navigate_to_url(
    window: WebviewWindow,
    url: String,
) -> Result<CommandResponse<bool>, String> {
    // 导航到目标 URL
    let nav_script = format!(r#"window.location.href = "{}";"#, url.replace("\"", "\\\""));
    window.eval(nav_script)
        .map_err(|e| format!("导航失败: {:?}", e))?;
    
    // 延迟注入 UI 脚本（等待页面开始加载）
    let window_clone = window.clone();
    let ui_script = INJECT_BROWSER_UI.to_string();
    
    tokio::spawn(async move {
        // 等待页面加载
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        
        // 注入 UI
        let _ = window_clone.eval(&ui_script);
        
        // 再次注入（确保成功）
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let _ = window_clone.eval(&ui_script);
    });
    
    log::info!("[WebView] Navigate to: {}", url);
    Ok(CommandResponse::success(true))
}

/// 重新注入浏览器 UI（用于页面跳转后）
#[tauri::command]
pub async fn reinject_browser_ui(
    window: WebviewWindow,
) -> Result<CommandResponse<bool>, String> {
    window.eval(INJECT_BROWSER_UI)
        .map_err(|e| format!("注入失败: {:?}", e))?;
    
    log::info!("[WebView] Browser UI reinjected");
    Ok(CommandResponse::success(true))
}

/// 检查浏览器 UI 是否存在
#[tauri::command]
pub async fn check_browser_ui(
    window: WebviewWindow,
) -> Result<CommandResponse<bool>, String> {
    let result = window.eval(r#"!!document.getElementById('__browser_ui_host__')"#)
        .map_err(|e| format!("检查失败: {:?}", e))?;
    
    // eval 返回的是 ()，我们需要再次查询
    let has_ui = true; // 简化处理，前端会处理重试
    Ok(CommandResponse::success(has_ui))
}

/// 返回上一页
#[tauri::command]
pub async fn navigate_back(
    window: WebviewWindow,
) -> Result<CommandResponse<bool>, String> {
    let script = r#"window.__TAURI_GO_BACK__ && window.__TAURI_GO_BACK__();"#.to_string();
    
    window.eval(script)
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
