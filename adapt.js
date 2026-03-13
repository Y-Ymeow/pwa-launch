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
(function () {
  // 防止重复注入
  if (window.__TAURI_ADAPT_INJECTED__) return;
  window.__TAURI_ADAPT_INJECTED__ = true;

  console.log("[PWA Adapt] Initializing...");

  // 生成唯一ID
  const generateId = () =>
    Date.now().toString(36) + Math.random().toString(36).substr(2);

  // 先保存原始 fetch 和 XHR（必须在覆盖之前）
  const originalFetch = window.fetch.bind(window);
  const OriginalXHR = window.XMLHttpRequest;

  // 创建 Tauri 桥接对象
  const tauriBridge = {
    _ready: false,
    _pending: new Map(),

    // 平台检测：Android/iOS 使用 http://static.localhost，桌面端使用 static://localhost
    get _isMobile() {
      return /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(
        navigator.userAgent,
      );
    },

    // 获取 static 协议的基础 URL
    get staticBaseUrl() {
      return this._isMobile ? "http://static.localhost" : "static://localhost";
    },

    // 初始化 - 发送 ready 信号给父容器
    async init() {
      if (window.parent === window) {
        console.log("[PWA Adapt] Not in iframe");
        return false;
      }

      // 发送 ready 信号
      window.parent.postMessage({ type: "ADAPT_READY" }, "*");

      // 等待父容器确认
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

        // 超时
        setTimeout(() => {
          if (!this._ready) {
            window.removeEventListener("message", handler);
            console.log("[PWA Adapt] Timeout, forcing ready status");
            this._ready = true;
            resolve(false);
          }
        }, 3000); // 增加到 3 秒
      });
    },

    // 禁用 Service Worker (自定义协议不支持)
    _shimServiceWorker() {
      if ("serviceWorker" in navigator) {
        console.log(
          "[PWA Adapt] Shimming ServiceWorker to prevent protocol errors",
        );
        navigator.serviceWorker.register = function () {
          console.warn(
            "[PWA Adapt] ServiceWorker registration blocked (unsupported on custom protocol)",
          );
          return new Promise(() => {}); // 返回一个永远 pending 的 promise
        };
      }
    },

    // 调用 Tauri 命令
    async invoke(cmd, payload = {}) {

      if (window.__TAURI_INTERNALS__) {
        return window.__TAURI_INTERNALS__.invoke(cmd, payload);
      }

      return new Promise((resolve, reject) => {
        const id = generateId();

        // 设置超时
        const timeout = setTimeout(() => {
          this._pending.delete(id);
          reject(new Error("Invoke timeout"));
        }, 30000);

        // 存储回调
        this._pending.set(id, { resolve, reject, timeout });

        // 发送请求到父容器
        window.parent.postMessage(
          {
            type: "ADAPT_INVOKE",
            id,
            cmd,
            payload,
          },
          "*",
        );
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

    // 文件对话框支持
    async openFileDialog(options = {}) {
      const result = await this.invoke("open_file_dialog", {
        title: options.title,
        multiple: options.multiple,
        filters: options.filters,
        directory: options.directory,
      });

      if (result.success && result.data && result.data.paths) {
        return options.multiple ? result.data.paths : result.data.paths[0];
      }
      return null;
    },

    // 读取文件内容
    async readFileContent(path) {
      const result = await this.invoke("read_file_content", { path });

      if (result.success && result.data) {
        // 解码 base64
        const byteCharacters = atob(result.data.content);
        const bytes = new Uint8Array(byteCharacters.length);
        for (let i = 0; i < byteCharacters.length; i++) {
          bytes[i] = byteCharacters.charCodeAt(i);
        }
        const blob = new Blob([bytes], { type: result.data.mimeType });
        return {
          name: result.data.name,
          path: result.data.path,
          size: result.data.size,
          mimeType: result.data.mimeType,
          blob: blob,
        };
      }
      return null;
    },

    // 读取文件指定范围（用于获取元数据，避免读取整个大文件）
    async readFileRange(path, offset = 0, length = 262144) {
      // 默认读取 256KB（足够大部分 MP3/FLAC 的元数据）
      const result = await this.invoke("read_file_range", {
        path,
        offset,
        length,
      });

      if (result.success && result.data) {
        // 解码 base64
        const byteCharacters = atob(result.data.content);
        const bytes = new Uint8Array(byteCharacters.length);
        for (let i = 0; i < byteCharacters.length; i++) {
          bytes[i] = byteCharacters.charCodeAt(i);
        }
        return {
          name: result.data.name,
          path: result.data.path,
          size: result.data.size,
          offset: result.data.offset,
          length: result.data.length,
          bytes: bytes,
          arrayBuffer: bytes.buffer,
        };
      }
      return null;
    },

    // 获取本地文件 URL (static://localhost/... 或 http://static.localhost/...)
    // 调用 Rust 后端，由后端根据文件类型决定使用 local server（音视频）还是 static 协议
    async resolve_local_file_url(filePath) {
      // 防护：如果已经是 URL，直接返回
      if (
        filePath.startsWith("static://") ||
        filePath.startsWith("http://static.localhost/") ||
        filePath.startsWith("http://127.0.0.1:") ||
        filePath.startsWith("http://localhost:")
      ) {
        console.log(
          "[PWA Adapt] resolve_local_file_url: already URL, returning as-is:",
          filePath,
        );
        return filePath;
      }

      // 调用 Rust 后端，让后端决定使用 local server 还是 static 协议
      const result = await this.invoke("resolve_local_file_url", {
        path: filePath,
      });
      if (result.success && result.data) {
        console.log(
          "[PWA Adapt] resolve_local_file_url:",
          filePath,
          "->",
          result.data,
        );
        return result.data;
      }
      throw new Error("Failed to resolve file URL");
    },

    // 选择并读取本地文件，返回 static://localhost URL（推荐）
    // 通过宿主容器的 dialog 获取真实路径，然后转换为 static:// URL
    async pick_and_resolve_local_file(options = {}) {
      // 1. 调用宿主容器的 open_file_dialog 获取真实路径
      const result = await this.invoke("open_file_dialog", {
        title: options.title || "Select File",
        multiple: options.multiple || false,
        filters:
          options.types?.map((t) => ({
            name: t.description || "Files",
            extensions: Object.values(t.accept || {}).flat(),
          })) || [],
        directory: false,
      });

      if (!result.success || !result.data || result.data.paths.length === 0) {
        throw new Error("No file selected");
      }

      const paths = result.data.paths;

      // 2. 将路径转换为 URL，返回 {path, url} 对象
      if (options.multiple) {
        const items = await Promise.all(
          paths.map(async (p) => ({
            path: p,
            url: await this.resolve_local_file_url(p),
          })),
        );
        return items.filter((i) => i.url);
      } else {
        return {
          path: paths[0],
          url: await this.resolve_local_file_url(paths[0]),
        };
      }
    },

    // 获取文件信息（通过路径读取文件内容）
    async get_file_info(filePath) {
      const result = await this.invoke("read_file_content", { path: filePath });
      if (result.success && result.data) {
        // 解码 base64
        const byteCharacters = atob(result.data.content);
        const bytes = new Uint8Array(byteCharacters.length);
        for (let i = 0; i < byteCharacters.length; i++) {
          bytes[i] = byteCharacters.charCodeAt(i);
        }
        const blob = new Blob([bytes], { type: result.data.mimeType });
        return {
          name: result.data.name,
          path: result.data.path,
          size: result.data.size,
          mimeType: result.data.mimeType,
          blob: blob,
        };
      }
      return null;
    },

    // 完整的文件系统 API
    fs: {
      async readDir(path) {
        const res = await tauriBridge.invoke("fs_read_dir", { path });
        return res.success ? res.data : [];
      },
      async readFile(path, options = {}) {
        const res = await tauriBridge.invoke("read_file_content", { path });
        if (!res.success || !res.data)
          throw new Error(res.error || "Read failed");

        if (options.encoding === "utf8") {
          return atob(res.data.content);
        }
        // 返回 Uint8Array
        const binary = atob(res.data.content);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
        return bytes;
      },
      async writeFile(path, content, options = {}) {
        let payload = content;
        let isBinary = true;

        if (typeof content === "string") {
          if (options.encoding === "utf8") {
            isBinary = false;
          } else {
            // 如果是字符串但未指定 utf8，视为 base64
            payload = content;
          }
        } else if (
          content instanceof Uint8Array ||
          content instanceof ArrayBuffer
        ) {
          // 转为 base64 发送
          const bytes =
            content instanceof ArrayBuffer ? new Uint8Array(content) : content;
          let binary = "";
          for (let i = 0; i < bytes.byteLength; i++)
            binary += String.fromCharCode(bytes[i]);
          payload = btoa(binary);
        }

        const res = await tauriBridge.invoke("fs_write_file", {
          path,
          content: payload,
          isBinary,
        });
        if (!res.success) throw new Error(res.error || "Write failed");
        return true;
      },
      async createDir(path, options = {}) {
        const res = await tauriBridge.invoke("fs_create_dir", {
          path,
          recursive: options.recursive || false,
        });
        if (!res.success) throw new Error(res.error || "Create dir failed");
        return true;
      },
      async removeFile(path) {
        const res = await tauriBridge.invoke("fs_remove", {
          path,
          recursive: false,
        });
        if (!res.success) throw new Error(res.error || "Remove failed");
        return true;
      },
      async removeDir(path, options = {}) {
        const res = await tauriBridge.invoke("fs_remove", {
          path,
          recursive: options.recursive || false,
        });
        if (!res.success) throw new Error(res.error || "Remove failed");
        return true;
      },
      async exists(path) {
        const res = await tauriBridge.invoke("fs_exists", { path });
        return res.success ? res.data : false;
      },
    },

    // 持久化 KV 存储 (替代不可靠的 localStorage)
    storage: {
      // 获取当前 PWA 的 ID (从 URL 或 Cookie 中提取)
      get _appId() {
        // 1. 优先从 cookie 获取 (后端设置的，最稳定)
        try {
          const contextCookie = document.cookie
            .split(";")
            .find((c) => c.trim().startsWith("pwa_context="));
          if (contextCookie) {
            const ctx = contextCookie.trim().substring("pwa_context=".length);
            // ctx 格式为 "https/domain.com"，取域名部分作为 ID
            return ctx.split("/")[1] || ctx;
          }
        } catch (e) {}

        // 2. 尝试从 URL 提取
        const url = window.location.href;
        // 支持多种格式: pwa-resource://localhost/https/domain.com/ 或 http://pwa-resource.localhost/https/domain.com/
        const match = url.match(/\/(https|http)\/([^/]+)/);
        if (match) return match[2];

        // 3. 兜底
        return window.location.hostname || "default";
      },
      async getItem(key) {
        const res = await tauriBridge.invoke("kv_get", {
          appId: this._appId,
          key,
        });
        return res.success ? res.data : null;
      },
      async setItem(key, value) {
        const res = await tauriBridge.invoke("kv_set", {
          appId: this._appId,
          key,
          value: String(value),
        });
        return res.success;
      },
      async removeItem(key) {
        const res = await tauriBridge.invoke("kv_remove", {
          appId: this._appId,
          key,
        });
        return res.success;
      },
      async clear() {
        const res = await tauriBridge.invoke("kv_clear", {
          appId: this._appId,
        });
        return res.success;
      },
    },

    // Cookie 管理 API
    cookie: {
      async get(url) {
        const res = await tauriBridge.invoke("get_cookies", { url });
        return res.success ? res.data : {};
      },
      async set(url, cookies) {
        // cookies 可以是 "key=value" 字符串或对象
        const res = await tauriBridge.invoke("set_cookies", { url, cookies });
        return res.success;
      },
    },

    // WebView 控制 API
    webview: {
      async open(options) {
        return await tauriBridge.invoke("open_webview", {
          url: options.url,
          title: options.title || "New Window",
          width: options.width || 1000,
          height: options.height || 800,
          injectAdapt: options.injectAdapt !== false,
        });
      },
      async close() {
        return await tauriBridge.invoke("close_current_webview");
      },
    },

    // 存储权限检查 API (Android)
    permission: {
      async checkStorage() {
        const res = await tauriBridge.invoke("check_storage_permission");
        return res.success ? res.data : { granted: false, can_request: false };
      },
      async requestStorage() {
        const res = await tauriBridge.invoke("request_storage_permission");
        return res.success;
      },
      // 检查并引导用户授权（带弹窗提示）
      async checkAndRequestStorage(message) {
        const status = await this.checkStorage();
        if (status.granted) {
          return true;
        }
        
        // 显示引导弹窗
        const msg = message || "需要授予所有文件访问权限才能使用此功能。请在设置中开启权限。";
        if (confirm(msg)) {
          await this.requestStorage();
        }
        return false;
      },
    },

    // 拦截 fetch - 支持 tauri:// 协议和跨域代理
    async fetch(url, options = {}) {
      const urlStr = url.toString();

      // tauri:// 协议调用
      if (urlStr.startsWith("tauri://")) {
        const match = urlStr.match(/tauri:\/\/(.+)/);
        if (match) {
          const api = match[1];

          // 文件对话框特殊处理
          if (api === "dialog/open") {
            const result = await this.openFileDialog(options);
            return new Response(JSON.stringify(result), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          }

          const result = await this.invoke(api, options);
          return new Response(JSON.stringify(result), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });
        }
      }

      try {
        const result = await this.invoke("proxy_fetch", {
          url: urlStr,
          method: options.method || "GET",
          headers: options.headers || {},
          body: options.body || null,
          responseType: options.responseType || "text",
        });

        const responseData = result.data || result;
        console.log(result)

        // 根据 responseType 处理响应体
        let body = responseData.body;
        const respType = options.responseType || "text";

        if (responseData.is_base64 || respType === "arraybuffer" || respType === "blob") {
          // base64 解码为 Uint8Array
          const byteCharacters = atob(body);
          const byteArray = new Uint8Array(byteCharacters.length);
          for (let i = 0; i < byteCharacters.length; i++) {
            byteArray[i] = byteCharacters.charCodeAt(i);
          }

          if (respType === "arraybuffer") {
            body = byteArray.buffer;
          } else {
            // blob 或默认二进制
            body = new Blob([byteArray], {
              type: responseData.headers["content-type"] || "application/octet-stream",
            });
          }
        }

        return new Response(body, {
          status: responseData.status,
          headers: responseData.headers,
          type: responseData.response_type
        });
      } catch (invokeError) {
        console.error("[PWA Adapt] Proxy fetch command failed:", invokeError);
        // 返回一个合成的错误响应，防止前端无限重试
        return new Response(JSON.stringify({ error: invokeError.message }), {
          data: null,
          status: 500,
          statusText: "Bad Gateway (Tauri Proxy Error)",
          headers: { "Content-Type": "application/json" },
        });
      }
    },
  };

  // 监听父容器响应
  window.addEventListener("message", (e) => {
    if (e.data?.type === "ADAPT_RESPONSE") {
      tauriBridge._handleResponse(e.data);
    }
    // 处理拖拽文件事件
    if (e.data?.type === "FILE_DROPPED") {
      window.dispatchEvent(
        new CustomEvent("tauri-file-dropped", { detail: e.data.files }),
      );
    }
  });

  // 暴露全局对象
  window.__TAURI__ = tauriBridge;
  window.tauri = tauriBridge;
  window.resolve_local_file_url =
    tauriBridge.resolve_local_file_url.bind(tauriBridge);

  // 覆盖 fetch - 但排除特殊协议避免死循环
  window.fetch = async function (url, ...rest) {
    const urlStr = url.toString();

    // 不拦截这些特殊协议
    if (
      urlStr.startsWith("ipc://") ||
      urlStr.startsWith("tauri://") ||
      urlStr.startsWith("data:") ||
      urlStr.startsWith("blob:") ||
      urlStr.startsWith("javascript:")
    ) {
      return originalFetch(url, ...rest);
    }

    // // 如果 tauri 未就绪，等待它
    // if (!tauriBridge._ready) {
    //   console.log("[PWA Adapt] Waiting for bridge before fetch...");
    //   let attempts = 0;
    //   while (!tauriBridge._ready && attempts < 50) {
    //     await new Promise((resolve) => setTimeout(resolve, 100));
    //     attempts++;
    //   }
    //   if (!tauriBridge._ready) {
    //     console.error("[PWA Adapt] Bridge timeout, using native fetch");
    //     return originalFetch(url, ...rest);
    //   }
    // }

    // tauriBridge.fetch 内部使用的是 originalFetch，不会递归
    return tauriBridge.fetch(url, ...rest);
  };

  // 拦截 XMLHttpRequest (axios 等库使用) - 立即执行确保 axios 加载前生效
  (function setupXHRProxy() {
    window.XMLHttpRequest = function () {
      const xhr = new OriginalXHR();
      const originalOpen = xhr.open.bind(xhr);
      const originalSend = xhr.send.bind(xhr);
      const originalSetRequestHeader = xhr.setRequestHeader.bind(xhr);
      const originalGetResponseHeader = xhr.getResponseHeader.bind(xhr);
      const originalGetAllResponseHeaders = xhr.getAllResponseHeaders.bind(xhr);

      let requestUrl = "";
      let requestMethod = "GET";
      let requestHeaders = {};
      let requestBody = null;
      let responseHeaders = {};
      let responseUrl = "";

      xhr.open = function (method, url, async = true, user, password) {
        requestMethod = method;
        requestUrl = url.toString();
        responseUrl = requestUrl;
        return originalOpen(method, url, async, user, password);
      };

      xhr.setRequestHeader = function (header, value) {
        requestHeaders[header] = value;
        return originalSetRequestHeader(header, value);
      };

      // 覆盖 getResponseHeader 以返回代理的响应头
      xhr.getResponseHeader = function (header) {
        if (responseHeaders[header] !== undefined) {
          return responseHeaders[header];
        }
        return originalGetResponseHeader(header);
      };

      // 覆盖 getAllResponseHeaders 以返回代理的响应头
      xhr.getAllResponseHeaders = function () {
        if (Object.keys(responseHeaders).length > 0) {
          return Object.entries(responseHeaders)
            .map(([k, v]) => `${k}: ${v}`)
            .join("\r\n");
        }
        return originalGetAllResponseHeaders();
      };

      xhr.send = async function (body) {
        requestBody = body;

        // 检测跨域请求
        try {
          const urlObj = new URL(requestUrl, window.location.href);
          if (urlObj.origin !== window.location.origin) {
            console.log("[PWA Adapt] XHR proxy:", requestUrl);

            // 如果 bridge 未就绪，等待它
            if (!tauriBridge._ready) {
              console.log(
                "[PWA Adapt] Waiting for bridge before proxying XHR...",
              );
              let attempts = 0;
              while (!tauriBridge._ready && attempts < 50) {
                await new Promise((resolve) => setTimeout(resolve, 100));
                attempts++;
              }
              if (!tauriBridge._ready) {
                console.error(
                  "[PWA Adapt] Bridge timeout, falling back to native XHR",
                );
                return originalSend(body);
              }
            }

            try {
              const result = await tauriBridge.invoke("proxy_fetch", {
                url: requestUrl,
                method: requestMethod,
                headers: requestHeaders,
                body: requestBody,
              });

              const responseData = result.data || result;

              // 保存响应头
              responseHeaders = responseData.headers || {};

              // 根据 responseType 设置 response
              let responseValue = responseData.body;
              const responseType = xhr.responseType || "text";

              if (responseType === "json") {
                try {
                  responseValue = JSON.parse(responseData.body);
                } catch (e) {
                  responseValue = responseData.body;
                }
              }

              // 模拟 XHR 响应
              Object.defineProperty(xhr, "status", {
                value: responseData.status,
                writable: false,
              });
              Object.defineProperty(xhr, "statusText", {
                value: responseData.status === 200 ? "OK" : "",
                writable: false,
              });
              Object.defineProperty(xhr, "responseText", {
                value: responseData.body,
                writable: false,
              });
              Object.defineProperty(xhr, "response", {
                value: responseValue,
                writable: false,
              });
              Object.defineProperty(xhr, "responseURL", {
                value: responseUrl,
                writable: false,
              });
              Object.defineProperty(xhr, "readyState", {
                value: 4,
                writable: false,
              });

              // 触发事件 - 按照 XHR 规范顺序
              if (xhr.onreadystatechange) {
                try {
                  xhr.onreadystatechange();
                } catch (e) {}
              }

              // 触发 load 事件
              if (xhr.onload) {
                try {
                  xhr.onload();
                } catch (e) {}
              }

              // 触发 loadend 事件
              if (xhr.onloadend) {
                try {
                  xhr.onloadend();
                } catch (e) {}
              }

              return;
            } catch (err) {
              console.error("[PWA Adapt] XHR proxy error:", err);
              Object.defineProperty(xhr, "status", {
                value: 500,
                writable: false,
              });
              Object.defineProperty(xhr, "statusText", {
                value: err.message,
                writable: false,
              });
              Object.defineProperty(xhr, "readyState", {
                value: 4,
                writable: false,
              });

              if (xhr.onerror) {
                try {
                  xhr.onerror(err);
                } catch (e) {}
              }
              if (xhr.onloadend) {
                try {
                  xhr.onloadend();
                } catch (e) {}
              }
              return;
            }
          }
        } catch (e) {}

        // 非跨域请求走原生 XHR
        return originalSend(body);
      };

      return xhr;
    };

    // 复制静态属性和方法
    Object.setPrototypeOf(window.XMLHttpRequest, OriginalXHR);
    window.XMLHttpRequest.prototype = OriginalXHR.prototype;

    console.log("[PWA Adapt] XHR proxy active");
  })();

  // 持久化 KV 存储 (替代不可靠的 localStorage)
  const storageManager = {
    // 获取当前 PWA 的唯一标识
    get appId() {
      try {
        const contextCookie = document.cookie
          .split(";")
          .find((c) => c.trim().startsWith("pwa_context="));
        if (contextCookie) {
          const ctx = contextCookie.trim().substring("pwa_context=".length);
          return ctx.replace(/\//g, "-").replace(/\./g, "_");
        }
      } catch (e) {}
      const match = window.location.href.match(/\/(https|http)\/([^/]+)/);
      return match ? `${match[1]}-${match[2].replace(/\./g, "_")}` : "default";
    },

    // 劫持 IndexedDB：给数据库名加前缀，实现域隔离
    hackIndexedDB() {
      const originalOpen = IDBFactory.prototype.open;
      const self = this;
      IDBFactory.prototype.open = function (name, version) {
        const prefixedName = `${self.appId}_${name}`;
        console.log(
          `[PWA Hack] IndexedDB isolation: ${name} -> ${prefixedName}`,
        );
        return originalOpen.call(this, prefixedName, version);
      };

      // 同时也劫持 deleteDatabase
      const originalDelete = IDBFactory.prototype.deleteDatabase;
      IDBFactory.prototype.deleteDatabase = function (name) {
        return originalDelete.call(this, `${self.appId}_${name}`);
      };
    },

    // 劫持 LocalStorage：自动同步到 SQLite
    async hackLocalStorage() {
      const appId = this.appId;
      console.log(`[PWA Hack] LocalStorage persistence active for: ${appId}`);

      // 1. 从 Rust 恢复所有数据
      try {
        const res = await tauriBridge.invoke("kv_get_all", { appId });
        if (res.success && res.data) {
          Object.entries(res.data).forEach(([key, value]) => {
            // 只有当本地没有或较旧时才覆盖（简单策略）
            if (!localStorage.getItem(key)) {
              localStorage.setItem(key, value);
            }
          });
          console.log(
            `[PWA Hack] Restored ${Object.keys(res.data).length} items from SQLite`,
          );
        }
      } catch (e) {
        console.error("[PWA Hack] Failed to restore storage:", e);
      }

      // 2. 劫持原生方法
      const originalSetItem = Storage.prototype.setItem;
      const originalRemoveItem = Storage.prototype.removeItem;
      const originalClear = Storage.prototype.clear;

      Storage.prototype.setItem = function (key, value) {
        originalSetItem.call(this, key, value);
        // 异步备份到 Rust
        tauriBridge
          .invoke("kv_set", { appId, key, value: String(value) })
          .catch(() => {});
      };

      Storage.prototype.removeItem = function (key) {
        originalRemoveItem.call(this, key);
        tauriBridge.invoke("kv_remove", { appId, key }).catch(() => {});
      };

      Storage.prototype.clear = function () {
        originalClear.call(this);
        tauriBridge.invoke("kv_clear", { appId }).catch(() => {});
      };
    },
  };

  // 自动初始化
  storageManager.hackIndexedDB();
  tauriBridge._shimServiceWorker();
  tauriBridge.init().then(() => {
    storageManager.hackLocalStorage();
    // 触发 ready 事件
    window.dispatchEvent(new CustomEvent("tauri-ready"));

    // 启动图片代理
    setupImageProxy();
  });

  // 图片代理：拦截所有 <img> 标签
  function setupImageProxy() {
    if (!tauriBridge._ready) return;

    if (!document.body) {
      console.warn(
        "[PWA Adapt] document.body not available, delaying image proxy...",
      );
      setTimeout(setupImageProxy, 100);
      return;
    }

    console.log("[PWA Adapt] Setting up image proxy...");

    // 处理单个图片
    async function proxyImage(img) {
      const src = img.src;
      if (!src || img.dataset.proxied) return;

      // 只处理外部图片
      try {
        const url = new URL(src, window.location.href);
        if (url.origin === window.location.origin) return;
        if (src.startsWith("blob:") || src.startsWith("data:")) return;
      } catch (e) {
        return;
      }

      img.dataset.proxied = "true";
      img.dataset.originalSrc = src;

      try {
        // 转换为 static 协议 URL，由 Rust 端代理
        // 格式: static://localhost/http://example.com/image.jpg (桌面端)
        // 格式: http://static.localhost/http://example.com/image.jpg (移动端)
        // Rust 端会检测路径是否以 http:// 或 https:// 开头，如果是则代理请求
        const staticUrl = `${tauriBridge.staticBaseUrl}/${src}`;
        img.src = staticUrl;
        console.log(
          "[PWA Adapt] Proxied image via static:",
          src.substring(0, 50),
        );
      } catch (err) {
        console.error("[PWA Adapt] Failed to proxy image:", src, err);
        img.dataset.proxied = "error";
      }
    }

    // 处理所有现有图片
    document.querySelectorAll("img").forEach(proxyImage);

    // 监听新添加的图片
    const observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeName === "IMG") {
            proxyImage(node);
          } else if (node.querySelectorAll) {
            node.querySelectorAll("img").forEach(proxyImage);
          }
        });
      });
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });

    console.log("[PWA Adapt] Image proxy active");
  }

  // Polyfill showOpenFilePicker for Tauri (使用桥接调用宿主 dialog)
  // 返回的 FileSystemFileHandle 包含真实路径，可通过 adapt.get_file_info 读取内容
  // 注意：强制覆盖原生 API，因为在 iframe/Tauri 环境中原生 API 不可用
  window.showOpenFilePicker = async function (options = {}) {
    // 等待 bridge 就绪
    if (!tauriBridge._ready) {
      console.log("[PWA Adapt] Waiting for bridge to be ready...");
      let attempts = 0;
      while (!tauriBridge._ready && attempts < 50) {
        await new Promise((resolve) => setTimeout(resolve, 100));
        attempts++;
      }
      if (!tauriBridge._ready) {
        throw new DOMException("Tauri bridge not ready", "NotAllowedError");
      }
    }

    // 调用宿主容器的文件选择器，获取 {path, url} 列表
    const result = await tauriBridge.pick_and_resolve_local_file(options);

    if (!result || (Array.isArray(result) && result.length === 0)) {
      throw new DOMException("No file selected", "AbortError");
    }

    const itemList = Array.isArray(result) ? result : [result];

    // 创建模拟的 FileSystemFileHandle 对象
    const handles = itemList.map((item) => {
      const filePath = item.path;
      const fileUrl = item.url;

      // 从路径中提取文件名
      const fileName = filePath.split(/[\\/]/).pop() || "file";

      console.log("[PWA Adapt] Selected file:", filePath, "->", fileUrl);

      return {
        kind: "file",
        name: fileName,
        _path: filePath,
        _url: fileUrl,
        getFile: async () => {
          const info = await tauriBridge.get_file_info(filePath);
          if (!info) {
            throw new Error(`Failed to read file: ${fileName}`);
          }
          const file = new File([info.blob], info.name, {
            type: info.mimeType,
          });
          file._path = filePath;
          return file;
        },
        // 返回后端提供的 URL（local server 或 static 协议）
        getURL: () => fileUrl,
        getPath: () => filePath,
      };
    });

    return handles;
  };

  console.log("[PWA Adapt] Bridge created, waiting for parent...");

  // ===== 验证助手：悬浮按钮 + Cookie 同步 =====
  function createVerifyAssistButton() {
    // 检查是否已存在
    if (document.getElementById("pwa-verify-assist-btn")) return;

    const btn = document.createElement("button");
    btn.id = "pwa-verify-assist-btn";
    btn.innerHTML = "✓ 验证完成";
    btn.style.cssText = `
      position: fixed !important;
      bottom: 20px !important;
      right: 20px !important;
      z-index: 2147483647 !important;
      padding: 12px 24px !important;
      background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important;
      color: white !important;
      border: none !important;
      border-radius: 25px !important;
      font-size: 14px !important;
      font-weight: bold !important;
      cursor: pointer !important;
      box-shadow: 0 4px 15px rgba(0,0,0,0.3) !important;
      transition: all 0.3s ease !important;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif !important;
    `;

    btn.addEventListener("mouseover", () => {
      btn.style.transform = "scale(1.05)";
      btn.style.boxShadow = "0 6px 20px rgba(0,0,0,0.4)";
    });

    btn.addEventListener("mouseout", () => {
      btn.style.transform = "scale(1)";
      btn.style.boxShadow = "0 4px 15px rgba(0,0,0,0.3)";
    });

    btn.onclick = async () => {
      try {
        // 收集所有 cookies
        const cookies = document.cookie;
        const url = window.location.href;
        const domain = window.location.hostname;

        console.log("[PWA Adapt] Syncing cookies for domain:", domain);

        // 发送到父容器
        window.parent.postMessage(
          {
            type: "ADAPT_SYNC_COOKIES",
            payload: {
              domain: domain,
              url: url,
              cookies: cookies,
              userAgent: navigator.userAgent,
            },
          },
          "*",
        );

        // 显示成功反馈
        btn.innerHTML = "✓ 已同步";
        btn.style.background =
          "linear-gradient(135deg, #11998e 0%, #38ef7d 100%) !important";

        setTimeout(() => {
          btn.innerHTML = "✓ 验证完成";
          btn.style.background =
            "linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important";
        }, 2000);
      } catch (err) {
        console.error("[PWA Adapt] Failed to sync cookies:", err);
        btn.innerHTML = "✗ 失败";
        btn.style.background =
          "linear-gradient(135deg, #eb3349 0%, #f45c43 100%) !important";

        setTimeout(() => {
          btn.innerHTML = "✓ 验证完成";
          btn.style.background =
            "linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important";
        }, 2000);
      }
    };

    // 延迟添加，确保页面加载完成
    setTimeout(() => {
      if (document.body) {
        document.body.appendChild(btn);
        console.log("[PWA Adapt] Verify assist button added");
      }
    }, 1000);
  }

  // 检测是否需要显示验证按钮（某些验证页面）
  function shouldShowVerifyButton() {
    // 检测常见的验证页面特征
    const title = document.title.toLowerCase();
    const bodyText = document.body?.textContent?.toLowerCase() || "";

    const verifyKeywords = [
      "just a moment",
      "checking your browser",
      "captcha",
      "验证码",
      "安全验证",
      "人机验证",
      "cloudflare",
      "please wait",
    ];

    return verifyKeywords.some(
      (kw) => title.includes(kw) || bodyText.includes(kw),
    );
  }

  // 自动检测并显示按钮
  function initVerifyAssist() {
    // 如果是验证页面，立即显示
    if (shouldShowVerifyButton()) {
      createVerifyAssistButton();
      return;
    }

    // 否则延迟检查（某些验证是异步加载的）
    setTimeout(() => {
      if (shouldShowVerifyButton()) {
        createVerifyAssistButton();
      }
    }, 3000);
  }

  // 启动验证助手
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", initVerifyAssist);
  } else {
    initVerifyAssist();
  }

  // 暴露全局 API，允许手动触发
  window.pwaVerifyAssist = {
    showButton: createVerifyAssistButton,
    syncCookies: () => {
      const btn = document.getElementById("pwa-verify-assist-btn");
      if (btn) btn.click();
    },
  };
})();
