/**
 * Core bridge functionality
 */

// 生成唯一ID
export const generateId = () =>
  Date.now().toString(36) + Math.random().toString(36).substr(2);

// 保存原始 fetch 和 XHR
export const originalFetch = window.fetch.bind(window);
export const OriginalXHR = window.XMLHttpRequest;

// 创建 Tauri 桥接对象
export function createBridge() {
  const bridge = {
    _ready: false,
    _pending: new Map(),

    get _isMobile() {
      return /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(
        navigator.userAgent,
      );
    },

    async init() {
      if (window.parent === window) {
        return false;
      }

      window.parent.postMessage({ type: "ADAPT_READY" }, "*");

      return new Promise((resolve) => {
        const handler = (e) => {
          if (e.data?.type === "ADAPT_PARENT_READY") {
            window.removeEventListener("message", handler);
            this._ready = true;
            console.log("[PWA Adapt] Ready!");
            resolve(true);
          }
        };
        window.addEventListener("message", handler);

        setTimeout(() => {
          if (!this._ready) {
            window.removeEventListener("message", handler);
            this._ready = true;
            resolve(false);
          }
        }, 3000);
      });
    },

    _shimServiceWorker() {
      // 不劫持 Service Worker，让 PWA 使用自己的 SW
      // 缓存通过 persistentCache API 手动管理
      console.log("[PWA Adapt] Service Worker not shimmed, using native SW");
    },

    async invoke(cmd, payload = {}) {
      // 生成唯一请求ID，用于匹配并发请求的响应
      const requestId = generateId();

      return new Promise((resolve, reject) => {
        window.parent.postMessage(
          { type: "ADAPT_INVOKE", cmd, payload, requestId },
          "*",
        );

        // 使用 requestId 匹配响应，避免并发请求错乱
        const handler = (e) => {
          if (
            e.data?.type === "ADAPT_RESULT" &&
            e.data.requestId === requestId
          ) {
            window.removeEventListener("message", handler);
            if (e.data.error) {
              reject(new Error(e.data.error));
            } else {
              resolve(e.data.result);
            }
          }
        };
        window.addEventListener("message", handler);

        // 30秒超时
        setTimeout(() => {
          window.removeEventListener("message", handler);
          reject(new Error("Invoke timeout"));
        }, 30000);
      });
    },

    _handleResponse(data) {
      // 简化后不再需要此方法
    },
  };

  return bridge;
}
