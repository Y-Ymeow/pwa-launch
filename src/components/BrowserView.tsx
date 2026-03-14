import { useState, useEffect, useCallback } from 'react';
import { invoke } from "@tauri-apps/api/core";
import type { BrowserHistoryItem, CommandResponse } from './types';

interface BrowserViewProps {
  browserUrl: string;
  setBrowserUrl: (url: string) => void;
  browserHistory: BrowserHistoryItem[];
  setBrowserHistory: (history: BrowserHistoryItem[] | ((prev: BrowserHistoryItem[]) => BrowserHistoryItem[])) => void;
  onClose: () => void;
  getProxiedUrl: (url: string) => string;
  showMessage: (type: "success" | "error", text: string) => void;
}

// 浏览器 UI 注入脚本 - 使用 Shadow DOM 隔离样式
const BROWSER_UI_SCRIPT = `
(function() {
  if (window.__BROWSER_UI_INJECTED__) return;
  window.__BROWSER_UI_INJECTED__ = true;
  
  const host = document.createElement('div');
  host.id = '__browser_ui_host__';
  host.style.cssText = 'position:fixed;top:0;left:0;right:0;z-index:2147483647;pointer-events:none;';
  document.documentElement.appendChild(host);
  
  const shadow = host.attachShadow({ mode: 'open' });
  
  shadow.innerHTML = \`
    <style>
      * { box-sizing: border-box !important; margin: 0 !important; padding: 0 !important; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif !important; }
      .browser-bar {
        background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%) !important;
        padding: 8px 12px !important;
        display: flex !important; align-items: center !important; gap: 8px !important;
        box-shadow: 0 2px 10px rgba(0,0,0,0.3) !important;
        pointer-events: auto !important; border-bottom: 1px solid rgba(255,255,255,0.1) !important;
      }
      .browser-btn {
        background: rgba(255,255,255,0.1) !important; border: none !important; color: white !important;
        width: 36px !important; height: 36px !important; border-radius: 8px !important;
        cursor: pointer !important; font-size: 18px !important;
        display: flex !important; align-items: center !important; justify-content: center !important;
        transition: all 0.2s !important; pointer-events: auto !important;
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
  \`;
  
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
`;

export function BrowserView({
  browserUrl,
  setBrowserUrl,
  browserHistory,
  setBrowserHistory,
  onClose,
  showMessage,
}: BrowserViewProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [inputUrl, setInputUrl] = useState(browserUrl || 'https://www.google.com');

  const navigateToUrl = useCallback(async (url: string) => {
    let finalUrl = url;
    if (!url.startsWith('http')) finalUrl = 'https://' + url;
    
    setBrowserUrl(finalUrl);
    setInputUrl(finalUrl);
    setIsLoading(true);
    
    try {
      // 先设置浏览器模式标志
      await invoke('eval_js', { script: 'window.__BROWSER_MODE__ = true;' });
      // 导航到 URL
      await invoke('navigate_to_url', { url: finalUrl });
      
      setBrowserHistory((prev) => 
        [{ url: finalUrl, title: finalUrl, timestamp: Date.now() }, ...prev.filter(h => h.url !== finalUrl)].slice(0, 50)
      );
    } catch (error) {
      showMessage('error', `导航失败: ${String(error)}`);
    } finally {
      setIsLoading(false);
    }
  }, [setBrowserUrl, setBrowserHistory, showMessage]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    navigateToUrl(inputUrl);
  };

  useEffect(() => {
    const handleMessage = async (e: MessageEvent) => {
      switch (e.data?.type) {
        case 'BROWSER_GO_BACK':
          onClose();
          break;
        case 'BROWSER_GO_HOME':
          onClose(); // 返回主页
          break;
        case 'BROWSER_NAVIGATE':
          if (e.data.url) navigateToUrl(e.data.url);
          break;
        case 'BROWSER_INSTALL':
          try {
            const response = await invoke<CommandResponse<{ name: string }>>("install_pwa", {
              request: { url: e.data.url || browserUrl },
            });
            if (response.success) {
              showMessage("success", `应用 "${response.data.name}" 安装成功！`);
            }
          } catch (error) {
            showMessage("error", `安装失败：${String(error)}`);
          }
          break;
      }
    };
    
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [browserUrl, navigateToUrl, onClose, showMessage]);

  // 初始导航
  useEffect(() => {
    if (browserUrl) {
      navigateToUrl(browserUrl);
    }
  }, []);

  // 定期检查并重新注入 UI（用于页面内跳转后）
  useEffect(() => {
    const checkInterval = setInterval(async () => {
      try {
        await invoke('reinject_browser_ui');
      } catch (e) {
        // 忽略错误
      }
    }, 3000);
    
    return () => clearInterval(checkInterval);
  }, []);

  return (
    <div className="browser-view">
      {/* 本地地址栏 */}
      <div className="browser-local-bar">
        <button onClick={onClose} className="browser-btn" title="返回应用列表">←</button>
        <form onSubmit={handleSubmit} className="browser-address-form">
          <input
            type="text"
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            placeholder="输入网址..."
            className="browser-address-input"
          />
          <button type="submit" className="browser-go-btn">→</button>
        </form>
      </div>

      {isLoading && (
        <div className="browser-loading">
          <div className="spinner"></div>
          <span>正在导航...</span>
        </div>
      )}

      {/* 快速访问 */}
      <div className="browser-quick-access">
        <h4>快速访问</h4>
        <div className="quick-links">
          <button onClick={() => navigateToUrl('https://www.google.com')}>Google</button>
          <button onClick={() => navigateToUrl('https://github.com')}>GitHub</button>
          <button onClick={() => navigateToUrl('https://www.bing.com')}>Bing</button>
        </div>
      </div>

      {/* 历史记录 */}
      {browserHistory.length > 0 && (
        <div className="browser-history-panel">
          <h4>最近访问</h4>
          {browserHistory.slice(0, 10).map((item, idx) => (
            <div key={idx} className="history-item" onClick={() => navigateToUrl(item.url)}>
              <span className="history-title">{item.title || item.url}</span>
              <span className="history-url">{item.url}</span>
            </div>
          ))}
        </div>
      )}

      <div className="browser-hint">
        <p>💡 提示：输入网址后，页面将在当前 WebView 中打开</p>
        <p>页面加载后会自动注入浏览器工具栏（地址栏、返回按钮等）</p>
      </div>
    </div>
  );
}