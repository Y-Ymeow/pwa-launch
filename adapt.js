/**
 * PWA Container Adapt Bridge
 * 
 * 将此脚本添加到 PWA 页面以使用 Tauri 原生功能：
 * <script src="adapt.js"></script>
 * 
 * 功能：
 * - 通过 postMessage 与父容器通信
 * - 代理 fetch 请求解决跨域
 * - 自动代理所有 <img> 标签
 * - 提供 window.__TAURI__.invoke() API
 */
(function() {
    // 防止重复注入
    if (window.__TAURI_ADAPT_INJECTED__) return;
    window.__TAURI_ADAPT_INJECTED__ = true;
    
    console.log('[PWA Adapt] Initializing...');
    
    // 生成唯一ID
    const generateId = () => Date.now().toString(36) + Math.random().toString(36).substr(2);
    
    // 先保存原始 fetch（必须在覆盖之前）
    const originalFetch = window.fetch.bind(window);
    
    // 创建 Tauri 桥接对象
    const tauriBridge = {
        _ready: false,
        _pending: new Map(),
        
        // 初始化 - 发送 ready 信号给父容器
        async init() {
            if (window.parent === window) {
                console.log('[PWA Adapt] Not in iframe');
                return false;
            }
            
            // 发送 ready 信号
            window.parent.postMessage({ type: 'ADAPT_READY' }, '*');
            
            // 等待父容器确认
            return new Promise((resolve) => {
                const handler = (e) => {
                    if (e.data?.type === 'ADAPT_PARENT_READY') {
                        window.removeEventListener('message', handler);
                        this._ready = true;
                        console.log('[PWA Adapt] Ready!');
                        resolve(true);
                    }
                };
                window.addEventListener('message', handler);
                
                // 超时
                setTimeout(() => {
                    window.removeEventListener('message', handler);
                    console.log('[PWA Adapt] Timeout, assuming ready');
                    this._ready = true;
                    resolve(false);
                }, 1000);
            });
        },
        
        // 调用 Tauri 命令
        async invoke(cmd, payload = {}) {
            if (!this._ready) {
                throw new Error('Tauri Adapt not ready. Call init() first or wait for tauri-ready event.');
            }
            
            return new Promise((resolve, reject) => {
                const id = generateId();
                
                // 设置超时
                const timeout = setTimeout(() => {
                    this._pending.delete(id);
                    reject(new Error('Invoke timeout'));
                }, 30000);
                
                // 存储回调
                this._pending.set(id, { resolve, reject, timeout });
                
                // 发送请求到父容器
                window.parent.postMessage({
                    type: 'ADAPT_INVOKE',
                    id,
                    cmd,
                    payload
                }, '*');
            });
        },
        
        // 处理父容器响应
        _handleResponse(data) {
            const { id, result, error } = data;
            const pending = this._pending.get(id);
            if (pending) {
                clearTimeout(pending.timeout);
                this._pending.delete(id);
                if (error) {
                    pending.reject(new Error(error));
                } else {
                    pending.resolve(result);
                }
            }
        },
        
        // 拦截 fetch - 支持 tauri:// 协议和跨域代理
        async fetch(url, options = {}) {
            const urlStr = url.toString();
            
            // tauri:// 协议调用
            if (urlStr.startsWith('tauri://')) {
                const match = urlStr.match(/tauri:\/\/(.+)/);
                if (match) {
                    const api = match[1];
                    const result = await this.invoke(api, options);
                    return new Response(JSON.stringify(result), {
                        status: 200,
                        headers: { 'Content-Type': 'application/json' }
                    });
                }
            }
            
            // 检测跨域请求，走 Tauri 代理
            try {
                const urlObj = new URL(urlStr, window.location.href);
                if (urlObj.origin !== window.location.origin) {
                    // 添加 Referer 来避免防盗链
                    const targetUrl = new URL(urlStr);
                    const proxyHeaders = {
                        ...(options.headers || {}),
                        'Referer': targetUrl.origin + '/'
                    };
                    
                    const result = await this.invoke('proxy_fetch', {
                        url: urlStr,
                        method: options.method || 'GET',
                        headers: proxyHeaders,
                        body: options.body || null
                    });
                    
                    // 注意：result 包含 {success, data}，实际数据在 result.data 中
                    const responseData = result.data || result;
                    
                    // 如果是 base64 图片，转为 blob
                    let body = responseData.body;
                    if (responseData.is_base64 && responseData.headers['content-type']) {
                        const byteCharacters = atob(body);
                        const byteNumbers = new Array(byteCharacters.length);
                        for (let i = 0; i < byteCharacters.length; i++) {
                            byteNumbers[i] = byteCharacters.charCodeAt(i);
                        }
                        const byteArray = new Uint8Array(byteNumbers);
                        body = new Blob([byteArray], { 
                            type: responseData.headers['content-type'] 
                        });
                    }
                    
                    return new Response(body, {
                        status: responseData.status,
                        headers: responseData.headers
                    });
                }
            } catch (e) {}
            
            // 同域请求走原生 fetch（使用保存的 originalFetch）
            return originalFetch(url, options);
        }
    };
    
    // 监听父容器响应
    window.addEventListener('message', (e) => {
        if (e.data?.type === 'ADAPT_RESPONSE') {
            tauriBridge._handleResponse(e.data);
        }
    });
    
    // 暴露全局对象
    window.__TAURI__ = tauriBridge;
    window.tauri = tauriBridge;
    
    // 覆盖 fetch - 但排除特殊协议避免死循环
    window.fetch = function(url, ...rest) {
        const urlStr = url.toString();
        
        // 不拦截这些特殊协议
        if (urlStr.startsWith('ipc://') || 
            urlStr.startsWith('tauri://') ||
            urlStr.startsWith('data:') ||
            urlStr.startsWith('blob:') ||
            urlStr.startsWith('javascript:')) {
            return originalFetch(url, ...rest);
        }
        
        // 如果 tauri 已就绪，使用 tauri bridge
        if (tauriBridge._ready) {
            // tauriBridge.fetch 内部使用的是 originalFetch，不会递归
            return tauriBridge.fetch(url, ...rest);
        }
        
        // 否则使用原生 fetch
        return originalFetch(url, ...rest);
    };
    
    // 自动初始化
    tauriBridge.init().then(() => {
        // 触发 ready 事件
        window.dispatchEvent(new CustomEvent('tauri-ready'));
        
        // 启动图片代理
        setupImageProxy();
    });
    
    // 图片代理：拦截所有 <img> 标签
    function setupImageProxy() {
        if (!tauriBridge._ready) return;
        
        console.log('[PWA Adapt] Setting up image proxy...');
        
        // 处理单个图片
        async function proxyImage(img) {
            const src = img.src;
            if (!src || img.dataset.proxied) return;
            
            // 只处理外部图片
            try {
                const url = new URL(src, window.location.href);
                if (url.origin === window.location.origin) return;
                if (src.startsWith('blob:') || src.startsWith('data:')) return;
            } catch (e) { return; }
            
            img.dataset.proxied = 'true';
            img.dataset.originalSrc = src;
            
            try {
                // 通过代理获取图片（使用 fetch 走桥接）
                const response = await fetch(src);
                if (!response.ok) throw new Error('Failed to load');
                
                const blob = await response.blob();
                const blobUrl = URL.createObjectURL(blob);
                
                img.src = blobUrl;
                console.log('[PWA Adapt] Proxied image:', src.substring(0, 50));
            } catch (err) {
                console.error('[PWA Adapt] Failed to proxy image:', src, err);
                img.dataset.proxied = 'error';
            }
        }
        
        // 处理所有现有图片
        document.querySelectorAll('img').forEach(proxyImage);
        
        // 监听新添加的图片
        const observer = new MutationObserver((mutations) => {
            mutations.forEach((mutation) => {
                mutation.addedNodes.forEach((node) => {
                    if (node.nodeName === 'IMG') {
                        proxyImage(node);
                    } else if (node.querySelectorAll) {
                        node.querySelectorAll('img').forEach(proxyImage);
                    }
                });
            });
        });
        
        observer.observe(document.body, {
            childList: true,
            subtree: true
        });
        
        console.log('[PWA Adapt] Image proxy active');
    }
    
    console.log('[PWA Adapt] Bridge created, waiting for parent...');
})();