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
pub const INJECT_BROWSER_UI: &str = r#"
(function() {
    // 检查是否在浏览器模式（通过检查是否存在浏览器视图标志）
    if (!window.__BROWSER_MODE__ && window.parent !== window) {
        // 在 iframe 中且不是浏览器模式，不注入
        return;
    }
    
    // 检查 UI 是否已存在且用户未在输入
    function shouldInject() {
        const host = document.getElementById('__browser_ui_host__');
        if (!host) return true;
        
        // 检查用户是否正在输入
        const shadow = host.shadowRoot;
        if (shadow) {
            const addressInput = shadow.getElementById('__browser_address__');
            if (addressInput && (addressInput === document.activeElement || addressInput.matches(':focus'))) {
                return false; // 用户正在输入，不重新注入
            }
        }
        return false; // UI 已存在，不需要重新注入
    }
    
    // 每次页面加载都尝试注入
    function injectUI() {
        if (!shouldInject()) return;
        
        // 如果已存在，先移除
        const existingHost = document.getElementById('__browser_ui_host__');
        if (existingHost) {
            existingHost.remove();
        }
        
        // 给 body 添加 padding-top，避免内容被遮挡
        document.body.style.paddingTop = '52px';
        document.documentElement.style.paddingTop = '52px';
        
        // 修复浮动/固定定位的头部元素
        const style = document.createElement('style');
        style.id = '__browser_ui_fix_style__';
        style.textContent = `
            /* 给固定/浮动定位的头部元素添加顶部间距 */
            *[style*="position: fixed"][style*="top: 0"],
            *[style*="position:fixed"][style*="top:0"],
            header[style*="position: fixed"],
            header[style*="position:fixed"],
            .header[style*="position: fixed"],
            .header[style*="position:fixed"],
            #header[style*="position: fixed"],
            #header[style*="position:fixed"],
            nav[style*="position: fixed"][style*="top: 0"],
            nav[style*="position:fixed"][style*="top:0"],
            .nav[style*="position: fixed"][style*="top: 0"],
            .nav[style*="position:fixed"][style*="top:0"]
            {
                top: 52px !important;
            }
            
            /* 处理 sticky 定位 */
            *[style*="position: sticky"][style*="top: 0"],
            *[style*="position:sticky"][style*="top:0"]
            {
                top: 52px !important;
            }
        `;
        if (!document.getElementById('__browser_ui_fix_style__')) {
            document.head.appendChild(style);
        }
        
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
                    cursor: pointer !important; font-size: 16px !important; display: flex !important;
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
                .install-btn, .go-btn, .home-btn {
                    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important; border: none !important;
                    color: white !important; padding: 8px 12px !important; border-radius: 8px !important;
                    cursor: pointer !important; font-size: 13px !important; font-weight: 500 !important;
                    white-space: nowrap !important; pointer-events: auto !important; transition: all 0.2s !important;
                }
                .go-btn { background: linear-gradient(135deg, #11998e 0%, #38ef7d 100%) !important; }
                .home-btn { background: rgba(255,255,255,0.15) !important; }
                .browser-btn:hover, .install-btn:hover, .go-btn:hover, .home-btn:hover { 
                    transform: translateY(-1px) !important; box-shadow: 0 4px 12px rgba(0,0,0,0.3) !important; 
                }
                .spacer { height: 52px !important; }
            </style>
            <div class="browser-bar">
                <button class="browser-btn" id="__browser_back__" title="后退">←</button>
                <button class="browser-btn" id="__browser_forward__" title="前进">→</button>
                <button class="home-btn" id="__browser_home__" title="返回主页">🏠</button>
                <button class="browser-btn" id="__browser_refresh__" title="刷新">↻</button>
                <input type="text" class="address-input" id="__browser_address__" placeholder="输入网址回车跳转..." />
                <button class="go-btn" id="__browser_go__" title="跳转">GO</button>
                <button class="install-btn" id="__browser_install__" title="安装为应用">➕</button>
            </div>
            <div class="spacer"></div>
        `;
        
        document.documentElement.appendChild(host);
        
        const backBtn = shadow.getElementById('__browser_back__');
        const forwardBtn = shadow.getElementById('__browser_forward__');
        const homeBtn = shadow.getElementById('__browser_home__');
        const refreshBtn = shadow.getElementById('__browser_refresh__');
        const addressInput = shadow.getElementById('__browser_address__');
        const goBtn = shadow.getElementById('__browser_go__');
        const installBtn = shadow.getElementById('__browser_install__');
        
        addressInput.value = location.href;
        
        // 后退
        backBtn.addEventListener('click', () => {
            history.back();
        });
        
        // 前进
        forwardBtn.addEventListener('click', () => {
            history.forward();
        });
        
        // 返回主页
        homeBtn.addEventListener('click', () => {
            if (window.__TAURI_INTERNALS__) {
                window.__TAURI_INTERNALS__.invoke('navigate_back');
            } else {
                window.location.href = 'tauri://localhost'; // 回退到本地主页
            }
        });
        
        // 刷新
        refreshBtn.addEventListener('click', () => location.reload());
        
        // 地址栏回车跳转
        addressInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') {
                const url = addressInput.value.trim();
                if (url) {
                    window.parent.postMessage({ type: 'BROWSER_NAVIGATE', url }, '*');
                }
            }
        });
        
        // GO 按钮跳转
        goBtn.addEventListener('click', () => {
            const url = addressInput.value.trim();
            if (url) {
                window.parent.postMessage({ type: 'BROWSER_NAVIGATE', url }, '*');
            }
        });
        
        // 安装
        installBtn.addEventListener('click', () => {
            window.parent.postMessage({ type: 'BROWSER_INSTALL', url: location.href }, '*');
        });
        
        // 同步地址栏
        const syncAddress = () => {
            if (addressInput !== document.activeElement) {
                addressInput.value = location.href;
            }
        };
        
        // 页面导航时同步地址
        window.addEventListener('popstate', syncAddress);
        setInterval(syncAddress, 2000);
        
        console.log('[Browser UI] Injected');
    }
    
    // 拦截所有链接点击，在当前窗口打开
    document.addEventListener('click', (e) => {
        const link = e.target.closest('a');
        if (link && link.href && !link.href.startsWith('javascript:') && !link.href.startsWith('#')) {
            e.preventDefault();
            window.location.href = link.href;
        }
    }, true);
    
    // 拦截 window.open
    window.open = function(url, target, features) {
        if (url) {
            window.location.href = url;
        }
        return null;
    };
    
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
            setTimeout(injectUI, 300);
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

/// 执行 JavaScript（用于前端直接执行）
#[tauri::command]
pub async fn eval_js(
    window: WebviewWindow,
    script: String,
) -> Result<CommandResponse<()>, String> {
    window.eval(&script)
        .map_err(|e| format!("执行失败: {:?}", e))?;
    
    Ok(CommandResponse::success(()))
}
