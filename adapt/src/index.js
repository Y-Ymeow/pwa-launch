/**
 * PWA Container Adapt Bridge - Entry Point
 *
 * 将此脚本添加到 PWA 页面以使用 Tauri 原生功能：
 * <script src="adapt.min.js"></script>
 */

import { createBridge } from "./core.js";
import { createFS, setupFilePicker } from "./fs.js";
import {
  createNetwork,
  setupXHRProxy,
  setupImageProxy,
  getMediaProxyUrl,
  getImageProxyUrl,
} from "./network.js";
import { injectBrowserUI, initVerifyAssist } from "./ui.js";
import {
  playAudio,
  pauseAudio,
  resumeAudio,
  stopAudio,
  setAudioVolume,
  setAudioLoop,
  getAudioState,
  getAudioPosition,
  getAudioDuration,
  getAudioCurrentUrl,
  seekAudio,
  setAudioProgressCallback,
  AdaptAudio,
} from "./audio.js";

import {
  createSQL,
  createEAV,
  hijackLocalStorage as hijackLocalStorageSQL,
  createCache,
} from "./sql.js";

(function () {
  // 防止重复注入
  if (window.__TAURI_ADAPT_INJECTED__) return;
  window.__TAURI_ADAPT_INJECTED__ = true;

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
    } catch (e) {
      console.error("[PWA Adapt] Anti-detection failed:", e);
    }
  })();

  // 创建核心桥接
  const bridge = createBridge();

  // 创建功能模块
  const fs = createFS(bridge);
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
    getMediaProxyUrl: getMediaProxyUrl,

    /**
     * 选择音频文件（推荐用于音乐播放器）
     * 自动缓存本地服务器 URL，避免 Android content:// 授权过期问题
     * @param {Object} options - 配置选项
     * @returns {Promise<{name: string, path: string, url: string, sourceUrl: string}>}
     */
    async pickAudioFile(options = {}) {
      const result = await fs.pickAndResolveLocalFile({
        title: options.title || "选择音乐文件",
        multiple: false,
        types: [{
          description: "Audio files",
          accept: {
            'audio/*': ['.mp3', '.flac', '.wav', '.ogg', '.m4a', '.aac']
          }
        }]
      });

      if (!result || !result.url) {
        throw new Error("No file selected");
      }

      // 缓存 content:// URI 到本地 URL 的映射
      if (result.path.startsWith("content://")) {
        localStorage.setItem(`__file_url_${result.path}`, result.url);
        console.log("[Adapt] Cached local URL for:", result.path);
      }

      const fileName = result.path.split('/').pop().split('?')[0];

      return {
        name: decodeURIComponent(fileName),
        path: result.path,      // 原始路径（可能为 content://）
        url: result.url,        // 本地服务器 URL（持久化）
        sourceUrl: result.url   // 用于播放的 URL
      };
    },

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

    // WebView 控制（通过 postMessage 请求父窗口打开新标签）
    webview: {
      async open(options) {
        return new Promise((resolve, reject) => {
          const requestId = Date.now().toString(36) + Math.random().toString(36).substr(2, 9);
          const timeout = setTimeout(() => {
            reject(new Error("Open webview timeout"));
          }, 5000);

          const handler = (event) => {
            if (event.data?.type === "ADAPT_OPEN_WEBVIEW_RESPONSE" && event.data?.requestId === requestId) {
              clearTimeout(timeout);
              window.removeEventListener("message", handler);
              
              if (event.data.error) {
                reject(new Error(event.data.error));
              } else {
                resolve(event.data);
              }
            }
          };

          window.addEventListener("message", handler);
          window.parent.postMessage(
            { type: "ADAPT_OPEN_WEBVIEW", requestId, url: options.url },
            "*"
          );
        });
      },
    },

    // 音频播放（绕过 WebKitGTK GStreamer）
    audio: {
      play: playAudio,
      pause: pauseAudio,
      resume: resumeAudio,
      stop: stopAudio,
      setVolume: setAudioVolume,
      setLoop: setAudioLoop,
      getState: getAudioState,
      getPosition: getAudioPosition,
      getDuration: getAudioDuration,
      getCurrentUrl: getAudioCurrentUrl,
      seek: seekAudio,
      setProgressCallback: setAudioProgressCallback,
      AdaptAudio: AdaptAudio,
    },

    // 视频播放（使用 MPV）- 暂未实现
    video: {
      play: (url) => {
        console.warn("[PWA Adapt] Video play not implemented yet:", url);
        // 回退到使用 audio 播放（纯音频）
        return playAudio(url);
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

    // 同步当前页面 cookies 到 Rust CookieStore
    async syncCookies(domain) {
      const cookies = document.cookie;
      const targetDomain = domain || location.hostname;
      const userAgent = navigator.userAgent;

      return await invoke("sync_webview_cookies", {
        domain: targetDomain,
        cookies: cookies,
        userAgent: userAgent,
      });
    },

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

    // SQL 数据库（在 ADAPT_PARENT_READY 后初始化）
    sql: null,
    eav: null,
    
    // Cache API（在 ADAPT_PARENT_READY 后初始化）
    cache: null,
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
  setupFilePicker(fs, bridge);

  // 禁用 Service Worker
  tauriBridge._shimServiceWorker();

  // 标记是否已初始化
  let isStorageInitialized = false;
  
  // 主动请求获取 app 信息（验证机制）
  async function requestAppInfo() {
    return new Promise((resolve, reject) => {
      const requestId = Date.now().toString(36) + Math.random().toString(36).substr(2, 9);
      const timeout = setTimeout(() => {
        reject(new Error("Request app info timeout"));
      }, 5000);

      const handler = (event) => {
        if (event.data?.type === "ADAPT_APP_INFO_RESPONSE" && event.data?.requestId === requestId) {
          clearTimeout(timeout);
          window.removeEventListener("message", handler);
          
          if (event.data.error) {
            reject(new Error(event.data.error));
          } else {
            resolve(event.data.appId);
          }
        }
      };

      window.addEventListener("message", handler);
      window.parent.postMessage(
        { type: "ADAPT_GET_APP_INFO", requestId },
        "*"
      );
      console.log("[Adapt] Requesting app info...");
    });
  }

  // 初始化存储（使用父窗口验证后的 appId）
  async function initStorage() {
    if (isStorageInitialized) return;
    
    try {
      // 主动请求获取验证后的 appId
      const appId = await requestAppInfo();
      console.log(`[Adapt] Got verified appId: ${appId}`);
      
      // 初始化 SQL 接口（验证后的 id）
      tauriBridge.sql = createSQL(appId);
      tauriBridge.eav = createEAV(appId);
      
      // 劫持 LocalStorage（验证后的 id）
      hijackLocalStorageSQL(appId);
      
      // 初始化 Cache API（验证后的 id）
      tauriBridge.cache = createCache(appId);
      
      isStorageInitialized = true;
      console.log(`[Adapt] All storage initialized for ${appId}`);
      
      // 触发存储就绪事件
      window.dispatchEvent(new CustomEvent("adapt-storage-ready", { detail: { appId } }));
    } catch (error) {
      console.error("[Adapt] Failed to initialize storage:", error);
      // 使用 fallback（非安全模式，仅用于调试）
      const fallbackId = "unauthorized";
      tauriBridge.sql = createSQL(fallbackId);
      tauriBridge.eav = createEAV(fallbackId);
      hijackLocalStorageSQL(fallbackId);
      tauriBridge.cache = createCache(fallbackId);
    }
  }
  
  // 启动存储初始化
  initStorage();

  // 初始化
  tauriBridge.init().then(() => {
    // 劫持 window.open 走浏览器模式
    const originalOpen = window.open;
    window.open = function (url, target, features) {
      if (url && (url.startsWith("http://") || url.startsWith("https://"))) {
        tauriBridge.webview.open({ url }).catch((e) => {
          console.error("[PWA Adapt] webview.open failed:", e);
          // 失败时回退到原生 open
          return originalOpen.call(window, url, target, features);
        });
        return null; // window.open 通常返回新窗口引用，但这里不返回
      }
      return originalOpen.call(window, url, target, features);
    };

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
  window.get_media_proxy_url = tauriBridge.getMediaProxyUrl;
  window.getMediaProxyUrl = getMediaProxyUrl;
  window.getImageProxyUrl = getImageProxyUrl;
})();
