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
        console.log("[PWA Adapt] Not in iframe");
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
      if ("serviceWorker" in navigator) {
        navigator.serviceWorker.register = function () {
          return new Promise(() => {});
        };
      }
    },

    async invoke(cmd, payload = {}) {
      // 直接使用父窗口的 Tauri 能力
      return new Promise((resolve, reject) => {
        window.parent.postMessage(
          { type: "ADAPT_INVOKE", cmd, payload },
          "*",
        );
        
        // 简单的 one-time 监听
        const handler = (e) => {
          if (e.data?.type === "ADAPT_RESULT" && e.data.cmd === cmd) {
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
