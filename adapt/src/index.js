/**
 * PWA Container Adapt Bridge - Entry Point
 *
 * 将此脚本添加到 PWA 页面以使用 Tauri 原生功能：
 * <script src="adapt.min.js"></script>
 */

import {
  generateId,
  originalFetch,
  OriginalXHR,
  createBridge,
} from "./core.js";
import { createFS, setupFilePicker } from "./fs.js";
import { createStorage, hackIndexedDB, hackLocalStorage } from "./storage.js";
import { createNetwork, setupXHRProxy, setupImageProxy } from "./network.js";
import { injectBrowserUI, initVerifyAssist } from "./ui.js";

(function () {
  // 防止重复注入
  if (window.__TAURI_ADAPT_INJECTED__) return;
  window.__TAURI_ADAPT_INJECTED__ = true;

  console.log("[PWA Adapt] Initializing...");

  // ===== 反检测：隐藏 WebView 特征 =====
  (function antiDetect() {
    try {
      // 1. 覆盖 webdriver 标志
      Object.defineProperty(navigator, "webdriver", {
        get: () => undefined,
        configurable: true,
      });

      // 2. 覆盖 plugins（WebView 通常为空）
      Object.defineProperty(navigator, "plugins", {
        get: () => [
          { name: "Chrome PDF Plugin", filename: "internal-pdf-viewer" },
          {
            name: "Chrome PDF Viewer",
            filename: "mhjfbmdgcfjbbpaeojofohoefgiehjai",
          },
          { name: "Native Client", filename: "internal-nacl-plugin" },
        ],
        configurable: true,
      });

      // 3. 覆盖 mimeTypes
      Object.defineProperty(navigator, "mimeTypes", {
        get: () => [
          {
            type: "application/pdf",
            suffixes: "pdf",
            description: "Portable Document Format",
          },
          {
            type: "application/x-google-chrome-pdf",
            suffixes: "pdf",
            description: "Portable Document Format",
          },
        ],
        configurable: true,
      });

      // 4. 删除 chrome 对象上的 automation 标志
      if (window.chrome) {
        Object.defineProperty(window.chrome, "runtime", {
          get: () => ({
            OnInstalledReason: { CHROME_UPDATE: "chrome_update" },
            OnRestartRequiredReason: { APP_UPDATE: "app_update" },
          }),
          configurable: true,
        });
      }

      // 5. 覆盖 permissions API
      const originalQuery = navigator.permissions?.query;
      if (originalQuery) {
        navigator.permissions.query = function (parameters) {
          if (parameters.name === "notifications") {
            return Promise.resolve({ state: "prompt" });
          }
          return originalQuery.call(this, parameters);
        };
      }

      // 6. 伪造 notification 权限
      if (window.Notification) {
        Object.defineProperty(Notification, "permission", {
          get: () => "default",
          configurable: true,
        });
      }

      console.log("[PWA Adapt] Anti-detection applied");
    } catch (e) {
      console.error("[PWA Adapt] Anti-detection failed:", e);
    }
  })();

  // 创建核心桥接
  const bridge = createBridge();

  // 创建功能模块
  const fs = createFS(bridge);
  const storage = createStorage(bridge);
  const network = createNetwork(bridge);

  // 完整 API
  const tauriBridge = {
    ...bridge,

    // 文件系统
    fs,
    openFileDialog: fs.openFileDialog,
    readFileContent: fs.readFileContent,
    resolveLocalFileUrl: fs.resolveLocalFileUrl,
    pickAndResolveLocalFile: fs.pickAndResolveLocalFile,
    getFileInfo: fs.getFileInfo,
    readFileRange: fs.readFileRange,

    // 存储
    storage,

    // Cookie
    cookie: {
      async get(url) {
        const res = await bridge.invoke("get_cookies", { url });
        return res.success ? res.data : {};
      },
      async set(url, cookies) {
        const res = await bridge.invoke("set_cookies", { url, cookies });
        return res.success;
      },
    },

    // WebView 控制
    webview: {
      async open(options) {
        return await bridge.invoke("navigate_to_url", {
          url: options.url,
        });
      },
      async close() {
        return await bridge.invoke("navigate_back", {});
      },
    },

    // 权限
    permission: {
      async checkStorage() {
        const res = await bridge.invoke("check_storage_permission");
        return res.success ? res.data : { granted: false, can_request: false };
      },
      async requestStorage() {
        const res = await bridge.invoke("request_storage_permission");
        return res.success;
      },
      async checkAndRequestStorage(message) {
        const status = await this.checkStorage();
        if (status.granted) return true;
        const msg = message || "需要授予所有文件访问权限。请在设置中开启权限。";
        if (confirm(msg)) {
          await this.requestStorage();
        }
        return false;
      },
    },

    // 网络
    fetch: network.fetch,

    // 浏览器 UI 导航
    navigateTo(url) {
      // 保存当前 URL
      sessionStorage.setItem("__pwa_main_url", location.href);
      // 导航到新 URL
      location.href = url;
    },

    navigateBack() {
      const mainUrl = sessionStorage.getItem("__pwa_main_url");
      if (mainUrl) {
        location.href = mainUrl;
      } else {
        history.back();
      }
    },
  };

  // 暴露到全局
  window.__TAURI__ = tauriBridge;
  window.tauri = tauriBridge;
  window.__TAURI_BRIDGE__ = tauriBridge;

  // 覆盖 fetch
  window.fetch = tauriBridge.fetch;

  // 设置 XHR 代理
  setupXHRProxy(tauriBridge);

  // 设置文件选择器
  setupFilePicker(tauriBridge);

  // 劫持 IndexedDB
  hackIndexedDB();

  // 禁用 Service Worker
  tauriBridge._shimServiceWorker();

  // 初始化
  tauriBridge.init().then(() => {
    // 劫持 LocalStorage
    hackLocalStorage(bridge);

    // 启动图片代理
    setupImageProxy(tauriBridge);

    // 触发 ready 事件
    window.dispatchEvent(new CustomEvent("tauri-ready"));

    // 启动验证助手
    initVerifyAssist(tauriBridge);

    // 检查是否是外部跳转页面（有返回按钮需求）
    if (sessionStorage.getItem("__pwa_browser_mode")) {
      injectBrowserUI();
    }
  });

  // 监听父容器响应
  window.addEventListener("message", (e) => {
    if (e.data?.type === "ADAPT_RESPONSE") {
      bridge._handleResponse(e.data);
    }
    if (e.data?.type === "FILE_DROPPED") {
      window.dispatchEvent(
        new CustomEvent("tauri-file-dropped", { detail: e.data.files }),
      );
    }
  });

  // 暴露文件路径解析
  window.resolve_local_file_url = tauriBridge.resolveLocalFileUrl;

  console.log("[PWA Adapt] Bridge created, waiting for parent...");
})();
