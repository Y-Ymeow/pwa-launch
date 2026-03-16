use crate::models::CommandResponse;
use tauri::{AppHandle, Manager, WebviewWindow};

// 浏览器 UI 注入脚本 - 使用 Shadow DOM 隔离样式
pub const INJECT_BROWSER_UI: &str = r#"
(function() {
    // 检查是否应该注入
    if (!window.__BROWSER_MODE__ && window.parent !== window) {
        return;
    }
    if (window.location.href.indexOf('localhost') !== -1) {
        return;
    }
    
    // 存储状态
    let uiData = { bookmarks: [], history: [] };
    let dataLoaded = false;
    
    // 使用 appdata:// 协议访问全局数据（Android 需要用 http://appdata.localhost）
    // 由于 User-Agent 可能被修改，在 loadData 中动态检测
    let DATA_API_URL = 'appdata://localhost';
    
    // 从 KV 加载数据（使用与 App.tsx 相同的 key）
    async function loadData() {
        try {
            // 检测正确的 API URL
            try {
                await fetch('appdata://localhost', {
                    method: 'POST',
                    body: JSON.stringify({ action: 'get', app_id: 'browser', key: 'test' })
                });
                DATA_API_URL = 'appdata://localhost';
            } catch (e) {
                DATA_API_URL = 'http://appdata.localhost';
            }

            // 加载书签
            const bookmarksRes = await fetch(DATA_API_URL, {
                method: 'POST',
                body: JSON.stringify({ action: 'get', app_id: 'browser', key: 'bookmarks' })
            });
            const bookmarksResult = await bookmarksRes.json();
            if (bookmarksResult.success && bookmarksResult.data) {
                uiData.bookmarks = JSON.parse(bookmarksResult.data);
            }

            // 加载历史
            const historyRes = await fetch(DATA_API_URL, {
                method: 'POST',
                body: JSON.stringify({ action: 'get', app_id: 'browser', key: 'history' })
            });
            const historyResult = await historyRes.json();
            if (historyResult.success && historyResult.data) {
                uiData.history = JSON.parse(historyResult.data);
            }

            dataLoaded = true;
            console.log('[Browser UI] Data loaded:', uiData);
        } catch (e) {
            console.log('[Browser UI] Failed to load data:', e);
        }
    }

    // 保存数据到 KV（分开保存，与 App.tsx 保持一致）
    async function saveData() {
        try {
            console.log('[Browser UI] Saving data:', uiData);
            await fetch(DATA_API_URL, {
                method: 'POST',
                body: JSON.stringify({
                    action: 'set',
                    app_id: 'browser',
                    key: 'bookmarks',
                    value: JSON.stringify(uiData.bookmarks)
                })
            });
            await fetch(DATA_API_URL, {
                method: 'POST',
                body: JSON.stringify({
                    action: 'set',
                    app_id: 'browser',
                    key: 'history',
                    value: JSON.stringify(uiData.history)
                })
            });
        } catch (e) {
            console.log('[Browser UI] Failed to save data:', e);
        }
    }
    
    // 添加到历史
    async function addToHistory(url, title) {
        // 先加载最新数据，避免覆盖
        await loadData();
        // 检查是否已存在
        if (uiData.history.some(h => h.url === url)) {
            // 移到最前面
            uiData.history = uiData.history.filter(h => h.url !== url);
        }
        uiData.history.unshift({ url, title: title || url, timestamp: Date.now() });
        if (uiData.history.length > 100) uiData.history = uiData.history.slice(0, 100);
        await saveData();
    }
    
    // 智能 URL 处理
    function processUrl(url) {
        url = url.trim();
        if (!url) return '';
        if (url.startsWith('http://') || url.startsWith('https://')) return url;
        if (url.startsWith('localhost') || url.startsWith('127.') || /^\d+\.\d+\.\d+\.\d+/.test(url)) {
            return 'http://' + url;
        }
        return 'https://' + url;
    }
    
    // 检查 UI 是否需要注入
    function shouldInject() {
        const host = document.getElementById('__browser_ui_host__');
        if (!host) return true;
        const shadow = host.shadowRoot;
        if (shadow) {
            const addressInput = shadow.getElementById('__browser_address__');
            if (addressInput && (addressInput === document.activeElement || addressInput.matches(':focus'))) {
                return false;
            }
        }
        return false;
    }
    
    // 检测是否为移动设备
    function isMobile() {
        return window.innerWidth <= 768 || /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
    }
    
    // 注入 UI
    async function injectUI() {
        if (!shouldInject()) return;
        
        const existingHost = document.getElementById('__browser_ui_host__');
        if (existingHost) {
            existingHost.remove();
        }
        
        document.documentElement.style.paddingTop = isMobile() ? '48px' : '52px';
        
        // 修复固定定位元素
        const style = document.createElement('style');
        style.id = '__browser_ui_fix_style__';
        style.textContent = `
            body *[style*="position: fixed"][style*="top: 0"]:not([data-browser-ui]),
            body *[style*="position:fixed"][style*="top:0"]:not([data-browser-ui]),
            body *[style*="position: sticky"][style*="top: 0"]:not([data-browser-ui]),
            body *[style*="position:sticky"][style*="top:0"]:not([data-browser-ui]),
            body header:not([data-browser-ui]),
            body .header:not([data-browser-ui]),
            body #header:not([data-browser-ui]),
            body nav:not([data-browser-ui]),
            body .nav:not([data-browser-ui]) {
                top: ${isMobile() ? '48px' : '52px'} !important;
            }
            /* 移动端视口优化 */
            @media (max-width: 768px) {
                html, body {
                    overflow-x: auto !important;
                    -webkit-overflow-scrolling: touch !important;
                }
                meta[name="viewport"] {
                    content: "width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" !important;
                }
            }
        `;
        if (!document.getElementById('__browser_ui_fix_style__')) {
            document.head.appendChild(style);
        }
        
        // 设置移动端视口
        if (isMobile()) {
            let viewport = document.querySelector('meta[name="viewport"]');
            if (!viewport) {
                viewport = document.createElement('meta');
                viewport.name = 'viewport';
                document.head.appendChild(viewport);
            }
            viewport.content = 'width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no';
        }
        
        const host = document.createElement('div');
        host.id = '__browser_ui_host__';
        host.style.cssText = 'position:fixed;top:0 !important;left:0;right:0;z-index:2147483647;pointer-events:none;';
        
        const shadow = host.attachShadow({ mode: 'open' });
        
        const barHeight = isMobile() ? '48px' : '52px';
        const btnSize = isMobile() ? '36px' : '36px';
        const btnFontSize = isMobile() ? '14px' : '16px';
        
        shadow.innerHTML = `
            <style>
                * { box-sizing: border-box !important; margin: 0 !important; padding: 0 !important; 
                    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif !important; }
                .browser-bar {
                    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%) !important;
                    padding: ${isMobile() ? '6px 8px' : '8px 12px'} !important;
                    display: flex !important; flex-direction: column !important;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.3) !important; pointer-events: auto !important;
                    border-bottom: 1px solid rgba(255,255,255,0.1) !important;
                    min-height: ${barHeight} !important;
                }
                .browser-bar-row {
                    display: flex !important; align-items: center !important;
                    gap: ${isMobile() ? '4px' : '8px'} !important; width: 100% !important;
                }
                .browser-bar-row.secondary {
                    margin-top: 6px !important; padding-top: 6px !important;
                    border-top: 1px solid rgba(255,255,255,0.1) !important;
                    flex-wrap: wrap !important; gap: 6px !important;
                }
                .browser-btn {
                    background: rgba(255,255,255,0.1) !important; border: none !important; color: white !important;
                    width: ${btnSize} !important; height: ${btnSize} !important; border-radius: 8px !important;
                    cursor: pointer !important; font-size: ${btnFontSize} !important; display: flex !important;
                    align-items: center !important; justify-content: center !important; 
                    transition: all 0.2s !important; pointer-events: auto !important;
                    flex-shrink: 0 !important; min-width: ${btnSize} !important;
                }
                .browser-btn:hover { background: rgba(255,255,255,0.2) !important; }
                .browser-btn:disabled { opacity: 0.4 !important; cursor: not-allowed !important; }
                .browser-btn.nav-btn { font-size: 18px !important; }
                .browser-btn.function-btn { font-size: 18px !important; font-weight: bold !important; }
                .browser-btn.function-btn.active { background: rgba(102,126,234,0.5) !important; }
                .browser-btn.go-btn { 
                    background: linear-gradient(135deg, #11998e 0%, #38ef7d 100%) !important;
                    font-size: 12px !important; font-weight: bold !important;
                    width: auto !important; padding: 0 12px !important;
                }
                .browser-btn.cancel-btn {
                    background: rgba(239,68,68,0.3) !important; width: auto !important;
                    padding: 0 10px !important; font-size: 12px !important;
                }
                .browser-btn.function-item {
                    width: auto !important; padding: 6px 10px !important; font-size: 12px !important;
                    display: flex !important; align-items: center !important; gap: 4px !important;
                }
                .browser-btn.function-item.active { background: rgba(102,126,234,0.5) !important; }
                .address-input-wrapper {
                    flex: 1 !important; position: relative !important;
                }
                .address-input {
                    width: 100% !important; background: rgba(0,0,0,0.3) !important;
                    border: 1px solid rgba(255,255,255,0.1) !important; color: white !important;
                    padding: ${isMobile() ? '6px 10px' : '8px 12px'} !important; border-radius: 8px !important;
                    font-size: ${isMobile() ? '13px' : '14px'} !important; outline: none !important;
                    pointer-events: auto !important;
                }
                .address-input:focus { border-color: #667eea !important; box-shadow: 0 0 0 2px rgba(102,126,234,0.3) !important; }
                .suggestions-dropdown {
                    position: absolute !important; top: 100% !important; left: 0 !important; right: 0 !important;
                    margin-top: 4px !important; background: #1a1a2e !important; border-radius: 8px !important;
                    box-shadow: 0 4px 12px rgba(0,0,0,0.5) !important; max-height: 200px !important;
                    overflow-y: auto !important; z-index: 2147483648 !important; display: none !important;
                }
                .suggestions-dropdown.show { display: block !important; }
                .suggestion-item {
                    padding: 8px 12px !important; cursor: pointer !important; color: white !important;
                    font-size: 13px !important; border-bottom: 1px solid rgba(255,255,255,0.05) !important;
                    display: flex !important; align-items: center !important; gap: 8px !important;
                }
                .suggestion-item:hover { background: rgba(255,255,255,0.1) !important; }
                .suggestion-item:last-child { border-bottom: none !important; }
                .panel-overlay {
                    position: fixed !important; top: ${barHeight} !important; left: 0 !important; right: 0 !important;
                    bottom: 0 !important; background: rgba(0,0,0,0.8) !important; z-index: 2147483646 !important;
                    display: none !important; pointer-events: auto !important;
                }
                .panel-overlay.show { display: block !important; }
                .browser-panel {
                    position: fixed !important; top: ${barHeight} !important; left: 0 !important; right: 0 !important;
                    max-height: 70vh !important; background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%) !important;
                    border-bottom: 1px solid rgba(255,255,255,0.1) !important; z-index: 2147483647 !important;
                    display: none !important; flex-direction: column !important; pointer-events: auto !important;
                    box-shadow: 0 4px 20px rgba(0,0,0,0.5) !important;
                }
                .browser-panel.show { display: flex !important; }
                .panel-header {
                    display: flex !important; justify-content: space-between !important; align-items: center !important;
                    padding: 12px !important; border-bottom: 1px solid rgba(255,255,255,0.1) !important;
                }
                .panel-title { color: white !important; font-size: 14px !important; font-weight: 600 !important; }
                .panel-close {
                    background: none !important; border: none !important; color: rgba(255,255,255,0.6) !important;
                    font-size: 20px !important; cursor: pointer !important; padding: 4px !important;
                }
                .panel-close:hover { color: white !important; }
                .panel-content {
                    overflow-y: auto !important; max-height: calc(70vh - 50px) !important;
                }
                .panel-item {
                    display: flex !important; align-items: center !important; gap: 10px !important;
                    padding: 10px 12px !important; border-bottom: 1px solid rgba(255,255,255,0.05) !important;
                    cursor: pointer !important; color: white !important;
                }
                .panel-item:hover { background: rgba(255,255,255,0.05) !important; }
                .panel-item-icon { font-size: 16px !important; flex-shrink: 0 !important; }
                .panel-item-info { flex: 1 !important; min-width: 0 !important; }
                .panel-item-title { font-size: 13px !important; white-space: nowrap !important; overflow: hidden !important; text-overflow: ellipsis !important; }
                .panel-item-url { font-size: 11px !important; color: rgba(255,255,255,0.5) !important; white-space: nowrap !important; overflow: hidden !important; text-overflow: ellipsis !important; }
                .panel-item-time { font-size: 11px !important; color: rgba(255,255,255,0.4) !important; flex-shrink: 0 !important; }
                .panel-item-delete {
                    background: none !important; border: none !important; color: rgba(255,255,255,0.4) !important;
                    font-size: 14px !important; cursor: pointer !important; padding: 4px !important;
                }
                .panel-item-delete:hover { color: #f5576c !important; }
                .panel-empty {
                    padding: 40px !important; text-align: center !important; color: rgba(255,255,255,0.4) !important;
                    font-size: 14px !important;
                }
                .panel-actions {
                    display: flex !important; gap: 10px !important; align-items: center !important;
                }
                .panel-clear-btn {
                    background: rgba(239,68,68,0.2) !important; border: none !important; color: #f5576c !important;
                    padding: 4px 10px !important; border-radius: 4px !important; font-size: 12px !important;
                    cursor: pointer !important;
                }
                .panel-clear-btn:hover { background: rgba(239,68,68,0.3) !important; }
                .hidden { display: none !important; }
                /* 工具栏隐藏/显示 */
                .browser-bar.hidden { display: none !important; }
                .browser-bar.hidden ~ .panel-overlay,
                .browser-bar.hidden ~ .browser-panel { display: none !important; }
                /* 工具栏位置切换 */
                .browser-bar.position-bottom {
                    position: fixed !important; bottom: 0 !important; top: auto !important;
                    border-top: 1px solid rgba(255,255,255,0.1) !important;
                    border-bottom: none !important;
                }
                @media (max-width: 768px) {
                    .browser-bar-row.secondary { max-height: 80px !important; overflow-y: auto !important; }
                }
            </style>
            <div class="browser-bar" id="__browser_bar__">
                <div class="browser-bar-row primary" id="__browser_row_primary__">
                    <button class="browser-btn nav-btn" id="__browser_back__" title="后退">←</button>
                    <button class="browser-btn nav-btn" id="__browser_forward__" title="前进">→</button>
                    <button class="browser-btn nav-btn" id="__browser_home__" title="主页">⌂</button>
                    <div class="address-input-wrapper">
                        <input type="text" class="address-input" id="__browser_address__" placeholder="输入网址..." />
                        <div class="suggestions-dropdown" id="__browser_suggestions__"></div>
                    </div>
                    <button class="browser-btn go-btn" id="__browser_go__">GO</button>
                    <button class="browser-btn cancel-btn hidden" id="__browser_cancel__">取消</button>
                    <button class="browser-btn function-btn" id="__browser_functions__" title="更多">⋮</button>
                    <button class="browser-btn nav-btn" id="__browser_position__" title="切换位置">⇅</button>
                    <button class="browser-btn nav-btn" id="__browser_toggle__" title="隐藏">✕</button>
                </div>
                <div class="browser-bar-row secondary hidden" id="__browser_row_secondary__">
                    <button class="browser-btn function-item" id="__browser_bookmark_add__">⭐ 收藏</button>
                    <button class="browser-btn function-item" id="__browser_bookmarks__">📑 收藏夹</button>
                    <button class="browser-btn function-item" id="__browser_history__">🕐 历史</button>
                    <button class="browser-btn function-item" id="__browser_cookie__">🍪 同步</button>
                </div>
            </div>
            <div class="panel-overlay" id="__browser_panel_overlay__"></div>
            <div class="browser-panel" id="__browser_panel_bookmarks__">
                <div class="panel-header">
                    <span class="panel-title">📑 收藏夹</span>
                    <button class="panel-close" id="__browser_panel_bookmarks_close__">×</button>
                </div>
                <div class="panel-content" id="__browser_bookmarks_list__"></div>
            </div>
            <div class="browser-panel" id="__browser_panel_history__">
                <div class="panel-header">
                    <div class="panel-actions">
                        <button class="panel-clear-btn" id="__browser_history_clear__">清空</button>
                        <button class="panel-close" id="__browser_panel_history_close__">×</button>
                    </div>
                    <span class="panel-title">🕐 历史记录</span>
                </div>
                <div class="panel-content" id="__browser_history_list__"></div>
            </div>
            <div class="spacer" style="height: ${barHeight} !important;"></div>
        `;
        
        document.documentElement.appendChild(host);
        
        // 创建浮动按钮（在 Shadow DOM 外部）
        const floatBtn = document.createElement('button');
        floatBtn.id = '__browser_float_toggle__';
        floatBtn.className = 'browser-float-btn';
        floatBtn.title = '显示/隐藏工具栏';
        floatBtn.textContent = '⋮';
        floatBtn.style.cssText = `
            position: fixed !important; right: 10px !important;
            width: 36px !important; height: 36px !important;
            background: rgba(102,126,234,0.8) !important; border: none !important;
            border-radius: 50% !important; color: white !important;
            font-size: 16px !important; cursor: pointer !important;
            z-index: 2147483647 !important; display: none !important;
            align-items: center !important; justify-content: center !important;
            box-shadow: 0 2px 8px rgba(0,0,0,0.3) !important;
            pointer-events: auto !important;
        `;
        floatBtn.addEventListener('mouseover', () => {
            floatBtn.style.background = 'rgba(102,126,234,1)';
        });
        floatBtn.addEventListener('mouseout', () => {
            floatBtn.style.background = 'rgba(102,126,234,0.8)';
        });
        // 确保 body 存在再添加按钮
        if (document.body) {
            document.body.appendChild(floatBtn);
        } else {
            // 如果 body 还不存在，等待 DOMContentLoaded
            document.addEventListener('DOMContentLoaded', () => {
                document.body.appendChild(floatBtn);
            });
        }
        
        // 获取元素引用
        const els = {
            backBtn: shadow.getElementById('__browser_back__'),
            forwardBtn: shadow.getElementById('__browser_forward__'),
            homeBtn: shadow.getElementById('__browser_home__'),
            addressInput: shadow.getElementById('__browser_address__'),
            goBtn: shadow.getElementById('__browser_go__'),
            cancelBtn: shadow.getElementById('__browser_cancel__'),
            functionsBtn: shadow.getElementById('__browser_functions__'),
            positionBtn: shadow.getElementById('__browser_position__'),
            toggleBtn: shadow.getElementById('__browser_toggle__'),
            bar: shadow.getElementById('__browser_bar__'),
            floatBtn: floatBtn,
            primaryRow: shadow.getElementById('__browser_row_primary__'),
            secondaryRow: shadow.getElementById('__browser_row_secondary__'),
            suggestions: shadow.getElementById('__browser_suggestions__'),
            bookmarkAddBtn: shadow.getElementById('__browser_bookmark_add__'),
            bookmarksBtn: shadow.getElementById('__browser_bookmarks__'),
            historyBtn: shadow.getElementById('__browser_history__'),
            cookieBtn: shadow.getElementById('__browser_cookie__'),
            panelOverlay: shadow.getElementById('__browser_panel_overlay__'),
            bookmarksPanel: shadow.getElementById('__browser_panel_bookmarks__'),
            historyPanel: shadow.getElementById('__browser_panel_history__'),
            bookmarksList: shadow.getElementById('__browser_bookmarks_list__'),
            historyList: shadow.getElementById('__browser_history_list__'),
            bookmarksClose: shadow.getElementById('__browser_panel_bookmarks_close__'),
            historyClose: shadow.getElementById('__browser_panel_history_close__'),
            historyClear: shadow.getElementById('__browser_history_clear__'),
        };
        
        // 当前状态
        let isInputMode = false;
        let showSecondary = false;
        let activePanel = null;
        let isBarVisible = localStorage.getItem('__browser_bar_visible__') !== 'false';
        let barPosition = localStorage.getItem('__browser_bar_position__') || 'top';
        
        // 应用工具栏位置和显示状态
        function applyBarState() {
            if (isBarVisible) {
                els.bar.classList.remove('hidden');
                els.floatBtn.style.display = 'none';
                // document.documentElement.style.paddingTop = barPosition === 'top' ? (isMobile() ? 48 : 52) : 0;
                // document.documentElement.style.paddingBottom = barPosition === 'bottom' ? (isMobile() ? 48 : 52) : 0;
                
                if (barPosition === 'bottom') {
                    els.bar.classList.add('position-bottom');
                    els.floatBtn.style.top = '10px';
                    els.floatBtn.style.bottom = 'auto';
                } else {
                    els.bar.classList.remove('position-bottom');
                    els.floatBtn.style.top = 'auto';
                    els.floatBtn.style.bottom = '10px';
                }
            } else {
                els.bar.classList.add('hidden');
                els.floatBtn.style.display = 'flex';
                document.documentElement.style.paddingTop = '0';
                document.documentElement.style.paddingBottom = '0';
            }
        }
        
        // 切换工具栏显示/隐藏
        function toggleBar() {
            isBarVisible = !isBarVisible;
            localStorage.setItem('__browser_bar_visible__', isBarVisible);
            applyBarState();
        }
        
        // 切换工具栏位置
        function togglePosition() {
            barPosition = barPosition === 'top' ? 'bottom' : 'top';
            localStorage.setItem('__browser_bar_position__', barPosition);
            applyBarState();
        }
        
        // 更新 UI 状态
        function updateUI() {
            els.addressInput.value = location.href;
            
            if (isInputMode) {
                els.backBtn.classList.add('hidden');
                els.forwardBtn.classList.add('hidden');
                els.homeBtn.classList.add('hidden');
                els.functionsBtn.classList.add('hidden');
                els.positionBtn.classList.add('hidden');
                els.toggleBtn.classList.add('hidden');
                els.cancelBtn.classList.remove('hidden');
                els.secondaryRow.classList.add('hidden');
                showSecondary = false;
                els.functionsBtn.classList.remove('active');
            } else {
                els.backBtn.classList.remove('hidden');
                els.forwardBtn.classList.remove('hidden');
                els.homeBtn.classList.remove('hidden');
                els.functionsBtn.classList.remove('hidden');
                els.positionBtn.classList.remove('hidden');
                els.toggleBtn.classList.remove('hidden');
                els.cancelBtn.classList.add('hidden');
                els.secondaryRow.classList.toggle('hidden', !showSecondary);
                els.functionsBtn.classList.toggle('active', showSecondary);
            }
        }
        
        // 显示面板
        function showPanel(type) {
            activePanel = type;
            els.panelOverlay.classList.add('show');
            if (type === 'bookmarks') {
                renderBookmarks();
                els.bookmarksPanel.classList.add('show');
                els.historyPanel.classList.remove('show');
            } else if (type === 'history') {
                renderHistory();
                els.historyPanel.classList.add('show');
                els.bookmarksPanel.classList.remove('show');
            }
        }
        
        // 隐藏面板
        function hidePanels() {
            activePanel = null;
            els.panelOverlay.classList.remove('show');
            els.bookmarksPanel.classList.remove('show');
            els.historyPanel.classList.remove('show');
        }
        
        // 渲染收藏夹
        function renderBookmarks() {
            if (uiData.bookmarks.length === 0) {
                els.bookmarksList.innerHTML = '<div class="panel-empty">暂无收藏</div>';
                return;
            }
            els.bookmarksList.innerHTML = uiData.bookmarks.map((b, i) => `
                <div class="panel-item" data-url="${b.url}">
                    <span class="panel-item-icon">⭐</span>
                    <div class="panel-item-info">
                        <div class="panel-item-title">${b.title || b.url}</div>
                        <div class="panel-item-url">${b.url}</div>
                    </div>
                    <button class="panel-item-delete" data-index="${i}" title="删除">🗑️</button>
                </div>
            `).join('');
            
            // 绑定点击事件
            els.bookmarksList.querySelectorAll('.panel-item').forEach(item => {
                item.addEventListener('click', (e) => {
                    if (!e.target.classList.contains('panel-item-delete')) {
                        const url = item.getAttribute('data-url');
                        if (url) window.location.href = url;
                        hidePanels();
                    }
                });
            });
            els.bookmarksList.querySelectorAll('.panel-item-delete').forEach(btn => {
                btn.addEventListener('click', async (e) => {
                    e.stopPropagation();
                    // 先加载最新数据
                    await loadData();
                    const index = parseInt(btn.getAttribute('data-index'));
                    if (index >= 0 && index < uiData.bookmarks.length) {
                        uiData.bookmarks.splice(index, 1);
                        await saveData();
                        renderBookmarks();
                    }
                });
            });
        }
        
        // 渲染历史记录
        function renderHistory() {
            if (uiData.history.length === 0) {
                els.historyList.innerHTML = '<div class="panel-empty">暂无历史记录</div>';
                return;
            }
            els.historyList.innerHTML = uiData.history.slice(0, 50).map(h => `
                <div class="panel-item" data-url="${h.url}">
                    <span class="panel-item-icon">🌐</span>
                    <div class="panel-item-info">
                        <div class="panel-item-title">${h.title || h.url}</div>
                        <div class="panel-item-url">${h.url}</div>
                    </div>
                    <span class="panel-item-time">${new Date(h.timestamp).toLocaleDateString()}</span>
                </div>
            `).join('');
            
            els.historyList.querySelectorAll('.panel-item').forEach(item => {
                item.addEventListener('click', () => {
                    const url = item.getAttribute('data-url');
                    if (url) window.location.href = url;
                    hidePanels();
                });
            });
        }
        
        // 显示建议
        function showSuggestions() {
            const input = els.addressInput.value.toLowerCase();
            if (!input) {
                els.suggestions.classList.remove('show');
                return;
            }
            
            const allUrls = [...uiData.bookmarks.map(b => b.url), ...uiData.history.map(h => h.url)];
            const unique = Array.from(new Set(allUrls)).filter(url => 
                url.toLowerCase().includes(input) && url.toLowerCase() !== input
            ).slice(0, 5);
            
            if (unique.length === 0) {
                els.suggestions.classList.remove('show');
                return;
            }
            
            els.suggestions.innerHTML = unique.map(url => `
                <div class="suggestion-item" data-url="${url}">
                    <span>🌐</span><span>${url}</span>
                </div>
            `).join('');
            els.suggestions.classList.add('show');
            
            els.suggestions.querySelectorAll('.suggestion-item').forEach(item => {
                item.addEventListener('click', () => {
                    const url = item.getAttribute('data-url');
                    if (url) window.location.href = processUrl(url);
                    els.suggestions.classList.remove('show');
                });
            });
        }
        
        // 事件绑定
        els.backBtn.addEventListener('click', () => history.back());
        els.forwardBtn.addEventListener('click', () => history.forward());
        els.homeBtn.addEventListener('click', () => {
            window.location.href = window.__BASE_HOST__ || 'tauri://localhost';
        });
        
        els.goBtn.addEventListener('click', () => {
            const url = els.addressInput.value.trim();
            if (url) window.location.href = processUrl(url);
        });
        
        els.cancelBtn.addEventListener('click', () => {
            isInputMode = false;
            els.addressInput.value = location.href;
            els.suggestions.classList.remove('show');
            updateUI();
        });
        
        els.functionsBtn.addEventListener('click', () => {
            showSecondary = !showSecondary;
            updateUI();
        });
        
        // 工具栏位置切换按钮
        els.positionBtn.addEventListener('click', () => {
            togglePosition();
            showMessage(barPosition === 'top' ? '工具栏已置顶' : '工具栏已置底');
        });
        
        // 工具栏隐藏按钮
        els.toggleBtn.addEventListener('click', () => {
            toggleBar();
        });
        
        // 浮动按钮（显示工具栏）
        els.floatBtn.addEventListener('click', () => {
            toggleBar();
        });
        
        els.addressInput.addEventListener('focus', () => {
            isInputMode = true;
            updateUI();
            showSuggestions();
        });
        
        els.addressInput.addEventListener('input', showSuggestions);
        
        els.addressInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') {
                const url = els.addressInput.value.trim();
                if (url) {
                    window.location.href = processUrl(url);
                    els.suggestions.classList.remove('show');
                }
            }
        });
        
        els.addressInput.addEventListener('blur', () => {
            setTimeout(() => {
                els.suggestions.classList.remove('show');
            }, 200);
        });
        
        // 功能按钮
        els.bookmarkAddBtn.addEventListener('click', async () => {
            const url = location.href;
            // 先加载最新数据
            await loadData();
            if (!uiData.bookmarks.find(b => b.url === url)) {
                uiData.bookmarks.unshift({ url, title: document.title || url, createdAt: Date.now() });
                await saveData();
                showMessage('已添加到收藏');
            } else {
                showMessage('已收藏');
            }
        });
        
        els.bookmarksBtn.addEventListener('click', () => showPanel('bookmarks'));
        els.historyBtn.addEventListener('click', () => showPanel('history'));
        
        els.cookieBtn.addEventListener('click', () => {
            showMessage('Cookies 会在页面加载时自动同步');
        });
        
        els.bookmarksClose.addEventListener('click', hidePanels);
        els.historyClose.addEventListener('click', hidePanels);
        els.panelOverlay.addEventListener('click', hidePanels);
        
        els.historyClear.addEventListener('click', async () => {
            uiData.history = [];
            await saveData();
            renderHistory();
        });
        
        // 显示消息
        function showMessage(text) {
            const msg = document.createElement('div');
            msg.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:rgba(0,0,0,0.8);color:white;padding:8px 16px;border-radius:20px;font-size:13px;z-index:2147483649;';
            msg.textContent = text;
            shadow.appendChild(msg);
            setTimeout(() => msg.remove(), 2000);
        }
        
        // 同步地址栏
        setInterval(() => {
            if (els.addressInput !== document.activeElement) {
                els.addressInput.value = location.href;
            }
        }, 2000);
        
        // 监听 URL 变化
        let lastUrl = location.href;
        setInterval(() => {
            if (location.href !== lastUrl) {
                lastUrl = location.href;
                addToHistory(location.href, document.title).catch(() => {});
                els.addressInput.value = location.href;
            }
        }, 500);
        
        // 初始更新
        applyBarState();
        updateUI();
        await addToHistory(location.href, document.title);
        
        console.log('[Browser UI] Injected');
        
        // 加载数据
        await loadData();
    }
    
    // 立即注入
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', () => injectUI());
    } else {
        injectUI();
        console.log("[Browser UI] Injected");
    }
    
    // 监听 URL 变化
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
    window
        .eval(nav_script)
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
pub async fn reinject_browser_ui(window: WebviewWindow) -> Result<CommandResponse<bool>, String> {
    window
        .eval(INJECT_BROWSER_UI)
        .map_err(|e| format!("注入失败: {:?}", e))?;

    log::info!("[WebView] Browser UI reinjected");
    Ok(CommandResponse::success(true))
}

/// 检查浏览器 UI 是否存在
#[tauri::command]
pub async fn check_browser_ui(window: WebviewWindow) -> Result<CommandResponse<bool>, String> {
    let _result = window
        .eval(r#"!!document.getElementById('__browser_ui_host__')"#)
        .map_err(|e| format!("检查失败: {:?}", e))?;

    // eval 返回的是 ()，我们需要再次查询
    let has_ui = true; // 简化处理，前端会处理重试
    Ok(CommandResponse::success(has_ui))
}

/// 返回上一页
#[tauri::command]
pub async fn navigate_back(window: WebviewWindow) -> Result<CommandResponse<bool>, String> {
    let script = r#"window.__TAURI_GO_BACK__ && window.__TAURI_GO_BACK__();"#.to_string();

    window
        .eval(script)
        .map_err(|e| format!("返回失败: {:?}", e))?;

    log::info!("[WebView] Navigate back");
    Ok(CommandResponse::success(true))
}

/// 获取当前窗口的 WebView 列表（用于移动端获取 webview 引用）
#[tauri::command]
pub async fn get_webview_info(
    app: AppHandle,
) -> Result<CommandResponse<serde_json::Value>, String> {
    let windows: Vec<String> = app.webview_windows().keys().cloned().collect();

    Ok(CommandResponse::success(serde_json::json!({
        "windows": windows,
        "canGoBack": true,
        "canGoForward": true,
    })))
}

/// 执行 JavaScript（用于前端直接执行）
#[tauri::command]
pub async fn eval_js(window: WebviewWindow, script: String) -> Result<CommandResponse<()>, String> {
    window
        .eval(&script)
        .map_err(|e| format!("执行失败: {:?}", e))?;

    Ok(CommandResponse::success(()))
}
