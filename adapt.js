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

  // 先保存原始 fetch（必须在覆盖之前）
  const originalFetch = window.fetch.bind(window);

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
          window.removeEventListener("message", handler);
          console.log("[PWA Adapt] Timeout, assuming ready");
          this._ready = true;
          resolve(false);
        }, 1000);
      });
    },

    // 调用 Tauri 命令
    async invoke(cmd, payload = {}) {
      if (!this._ready) {
        throw new Error(
          "Tauri Adapt not ready. Call init() first or wait for tauri-ready event.",
        );
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
        title: options.title || "Select File",
        multiple: options.multiple || false,
        filters: options.filters || [],
        directory: options.directory || false,
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

    // 获取本地文件 URL (static://localhost/... 或 http://static.localhost/...)
    // 调用 Rust 后端，由后端根据文件类型决定使用 local server（音视频）还是 static 协议
    async resolve_local_file_url(filePath) {
      // 防护：如果已经是 URL，直接返回
      if (filePath.startsWith("static://") || 
          filePath.startsWith("http://static.localhost/") ||
          filePath.startsWith("http://127.0.0.1:") ||
          filePath.startsWith("http://localhost:")) {
        console.log("[PWA Adapt] resolve_local_file_url: already URL, returning as-is:", filePath);
        return filePath;
      }
      
      // 调用 Rust 后端，让后端决定使用 local server 还是 static 协议
      const result = await this.invoke('resolve_local_file_url', { path: filePath });
      if (result.success && result.data) {
        console.log("[PWA Adapt] resolve_local_file_url:", filePath, "->", result.data);
        return result.data;
      }
      throw new Error('Failed to resolve file URL');
    },

    // 选择并读取本地文件，返回 static://localhost URL（推荐）
    // 通过宿主容器的 dialog 获取真实路径，然后转换为 static:// URL
    async pick_and_resolve_local_file(options = {}) {
      // 1. 调用宿主容器的 open_file_dialog 获取真实路径
      // 参数需要包装在 options 键下
      const result = await this.invoke("open_file_dialog", {
        options: {
          title: options.title || "Select File",
          multiple: options.multiple || false,
          filters:
            options.types?.map((t) => ({
              name: t.description || "Files",
              extensions: Object.values(t.accept || {}).flat(),
            })) || [],
          directory: false,
        },
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

      // 检测跨域请求，走 Tauri 代理
      try {
        const urlObj = new URL(urlStr, window.location.href);
        if (urlObj.origin !== window.location.origin) {
          // 如果没有 Referer，自动添加
          const targetUrl = new URL(urlStr);
          const proxyHeaders = {
            ...(options.headers || {}),
          };
          if (!proxyHeaders["Referer"] && !proxyHeaders["referer"]) {
            proxyHeaders["Referer"] = targetUrl.origin + "/";
          }

          const result = await this.invoke("proxy_fetch", {
            url: urlStr,
            method: options.method || "GET",
            headers: proxyHeaders,
            body: options.body || null,
          });

          // 注意：result 包含 {success, data}，实际数据在 result.data 中
          const responseData = result.data || result;

          // 如果是 base64 图片，转为 blob
          let body = responseData.body;
          if (responseData.is_base64 && responseData.headers["content-type"]) {
            const byteCharacters = atob(body);
            const byteNumbers = new Array(byteCharacters.length);
            for (let i = 0; i < byteCharacters.length; i++) {
              byteNumbers[i] = byteCharacters.charCodeAt(i);
            }
            const byteArray = new Uint8Array(byteNumbers);
            body = new Blob([byteArray], {
              type: responseData.headers["content-type"],
            });
          }

          return new Response(body, {
            status: responseData.status,
            headers: responseData.headers,
          });
        }
      } catch (e) {}

      // 同域请求走原生 fetch（使用保存的 originalFetch）
      return originalFetch(url, options);
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
  window.fetch = function (url, ...rest) {
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
    window.dispatchEvent(new CustomEvent("tauri-ready"));

    // 启动图片代理
    setupImageProxy();
  });

  // 图片代理：拦截所有 <img> 标签
  function setupImageProxy() {
    if (!tauriBridge._ready) return;

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
})();
