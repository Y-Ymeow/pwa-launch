/**
 * Network APIs (fetch/XHR proxy)
 * 通过 postMessage 转发给父窗口代理请求
 */

import { originalFetch, OriginalXHR } from "./core.js";

// 生成唯一请求 ID
const generateRequestId = () =>
  Date.now().toString(36) + Math.random().toString(36).substr(2, 9);

// 本地服务器配置
const LOCAL_SERVER_PORT = 19315;

// 通过父窗口代理 HTTP 请求（postMessage -> 父窗口 fetch 本地服务器）
function proxyViaLocalServer(
  url,
  method,
  headers,
  body,
  isMedia = false,
  isXHR = false,
) {
  return new Promise((resolve, reject) => {
    const requestId = generateRequestId();

    const handler = (event) => {
      if (
        event.data?.type === "ADAPT_PROXY_RESPONSE" &&
        event.data?.requestId === requestId
      ) {
        window.removeEventListener("message", handler);

        if (event.data.success) {
          // 构造可多次读取的 Response 对象
          const responseData = event.data.data;
          let blob;

          if (responseData.isBase64) {
            // Base64 解码为二进制数据
            const binaryString = atob(responseData.body);
            const bytes = new Uint8Array(binaryString.length);
            for (let i = 0; i < binaryString.length; i++) {
              bytes[i] = binaryString.charCodeAt(i);
            }
            blob = new Blob([bytes]);
          } else {
            // 文本数据直接创建 Blob
            blob = new Blob([responseData.body]);
          }

          const responseHeaders = new Headers(responseData.headers);

          // 创建类 Response 对象，支持多次读取 body
          const response = {
            ok: responseData.status >= 200 && responseData.status < 300,
            status: responseData.status,
            statusText: responseData.statusText,
            headers: responseHeaders,
            url: url,
            clone: function () {
              return this;
            },
            text: async () => await blob.text(),
            json: async () => JSON.parse(await blob.text()),
            blob: async () => blob,
            arrayBuffer: async () => await blob.arrayBuffer(),
            bodyUsed: false,
          };
          resolve(response);
        } else {
          reject(new Error(event.data.error || "Proxy request failed"));
        }
      }
    };

    window.addEventListener("message", handler);

    // 30 秒超时
    setTimeout(() => {
      window.removeEventListener("message", handler);
      reject(new Error("Proxy request timeout"));
    }, 30000);

    // 转换 body 为字符串（处理 URLSearchParams/FormData）
    let bodyString = null;
    if (body !== null && body !== undefined) {
      if (typeof body === "string") {
        bodyString = body;
      } else if (body instanceof URLSearchParams) {
        bodyString = body.toString();
      } else if (body instanceof FormData) {
        // FormData 无法直接转换为字符串，需要特殊处理
        const entries = [];
        for (const [key, value] of body.entries()) {
          entries.push(
            `${encodeURIComponent(key)}=${encodeURIComponent(value)}`,
          );
        }
        bodyString = entries.join("&");
      } else {
        // 其他类型转为字符串
        bodyString = String(body);
      }
    }

    // 发送代理请求给父窗口，使用 isMedia/isXHR 标记
    window.parent.postMessage(
      {
        type: "ADAPT_PROXY_REQUEST",
        requestId,
        url,
        method,
        headers,
        body: bodyString,
        isMedia, // 标记是否为媒体请求
        isXHR, // 标记是否为 XHR 请求
      },
      "*",
    );
  });
}

export function createNetwork(bridge) {
  return {
    async fetch(input, init = {}) {
      // 处理 input 可能是 Request 对象的情况
      let urlStr;
      let options = { ...init };

      if (input instanceof Request) {
        urlStr = input.url;
        options.method = init.method || input.method;
        options.headers = init.headers || {};
        input.headers.forEach((value, key) => {
          if (!options.headers[key]) {
            options.headers[key] = value;
          }
        });
      } else {
        urlStr = input.toString();
      }
      console.log(options);

      // 检测是否是流式请求 (AI SSE/Stream)
      const isStreamRequest =
        options.headers?.["Accept"] === "text/event-stream" ||
        options.headers?.["accept"] === "text/event-stream" ||
        (typeof options.body === "string" &&
          options.body.includes('"stream":true')) ||
        (typeof options.body === "string" &&
          options.body.includes('"stream": true'));

      // 检测是否标记为直接请求
      const isDirectRequest =
        options.headers?.["X-Direct-Request"] === "true" ||
        options.headers?.["x-direct-request"] === "true";

      // 检测是否是音视频请求（走 /media/proxy 专用路由）
      const isMediaRequest =
        urlStr.match(
          /\.(mp3|m4a|ogg|wav|flac|aac|wma|mp4|webm|m4s|ts|m3u8|mpd)(\?.*)?$/i,
        ) ||
        options.headers?.["Accept"]?.startsWith("audio/") ||
        options.headers?.["Accept"]?.startsWith("video/");

      // 代理 HTTP/HTTPS 请求
      if (urlStr.startsWith("http://") || urlStr.startsWith("https://")) {
        // 直接请求，不走代理
        if (isDirectRequest) {
          const cleanOptions = { ...options };
          if (cleanOptions.headers) {
            delete cleanOptions.headers["X-Direct-Request"];
            delete cleanOptions.headers["x-direct-request"];
          }
          return await originalFetch(urlStr, cleanOptions);
        }

        // 流式请求特殊处理
        if (isStreamRequest) {
          return await originalFetch(urlStr, options);
        }

        // 音视频请求使用 /media/proxy 路由（禁用 gzip，流式传输）
        if (isMediaRequest) {
          // 设置 headers，自动添加 Referer
          const mediaHeaders = { ...options.headers };
          if (!mediaHeaders["Referer"] && !mediaHeaders["referer"]) {
            // try {
            //   const urlObj = new URL(urlStr);
            //   mediaHeaders["Referer"] = `${urlObj.protocol}//${urlObj.host}`;
            // } catch {
            //   mediaHeaders["Referer"] = location.href;
            // }
          }

          // 使用 postMessage 桥接，传递 isMedia=true
          return await proxyViaLocalServer(
            urlStr,
            options.method || "GET",
            mediaHeaders,
            options.body,
            true, // isMedia = true
            false, // isXHR = false (媒体请求不需要 X-Requested-With)
          );
        }

        // 通过父窗口代理请求（本地 HTTP 服务器）
        const headers = { ...options.headers };

        // 设置 Referer 为目标 URL 的基础部分（不带末尾斜杠，与 curl 一致）
        if (!headers["Referer"] && !headers["referer"]) {
          try {
            const urlObj = new URL(urlStr);
            headers["Referer"] = `${urlObj.protocol}//${urlObj.host}`;
          } catch {
            headers["Referer"] = location.href;
          }
        }

        // 使用简单的 User-Agent（与 curl 一致）
        headers["User-Agent"] = navigator.userAgent;

        return await proxyViaLocalServer(
          urlStr,
          options.method || "GET",
          headers,
          options.body,
          false, // isMedia
          false, // isXHR - 让服务器根据 Accept header 判断
        );
      }

      // 处理 tauri:// 协议
      if (urlStr.startsWith("tauri://")) {
        const match = urlStr.match(/tauri:\/\/(.+)/);
        if (match) {
          const api = match[1];

          if (api === "dialog/open") {
            const result = await bridge.openFileDialog?.(options);
            return new Response(JSON.stringify(result), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          }

          const result = await bridge.invoke(api, options);
          return new Response(JSON.stringify(result), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });
        }
      }

      // 未知协议
      console.error("[PWA Adapt] Unknown protocol:", urlStr);
      return new Response(
        JSON.stringify({ error: "Unknown protocol: " + urlStr }),
        {
          status: 400,
          statusText: "Bad Request",
          headers: { "Content-Type": "application/json" },
        },
      );
    },
  };
}

export function setupXHRProxy(tauriBridge) {
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

    xhr.open = function (method, url, async = true, user, password) {
      requestMethod = method;
      requestUrl = url.toString();
      console.log("create xhr");
      // 不调用 originalOpen，让 send 完全控制
      // 设置 readyState 为 OPENED (1)
      Object.defineProperty(xhr, "readyState", {
        value: 1,
        writable: true,
        configurable: true,
      });
      if (xhr.onreadystatechange) xhr.onreadystatechange();
      return;
    };

    xhr.setRequestHeader = function (header, value) {
      requestHeaders[header] = value;
      return originalSetRequestHeader(header, value);
    };

    xhr.getResponseHeader = function (header) {
      if (responseHeaders[header] !== undefined) {
        return responseHeaders[header];
      }
      return originalGetResponseHeader(header);
    };

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

      // 判断是否需要代理（跨域请求）
      let needProxy = false;
      let urlObj;

      try {
        urlObj = new URL(requestUrl, window.location.href);
        needProxy = urlObj.origin !== window.location.origin;
      } catch (e) {
        // URL 解析失败，直接走原生请求
        console.error("[PWA Adapt] Invalid URL:", requestUrl, e);
        return originalSend(body);
      }

      // 同域请求，直接走原生
      if (!needProxy) {
        return originalSend(body);
      }

      // 跨域请求，必须走代理
      console.log("[PWA Adapt] XHR proxy:", requestUrl);

      // 等待 bridge 就绪
      if (!tauriBridge._ready) {
        let attempts = 0;
        while (!tauriBridge._ready && attempts < 50) {
          await new Promise((resolve) => setTimeout(resolve, 100));
          attempts++;
        }
        if (!tauriBridge._ready) {
          console.error("[PWA Adapt] Bridge not ready");
          Object.defineProperty(xhr, "status", { value: 0, writable: false });
          Object.defineProperty(xhr, "readyState", {
            value: 4,
            writable: false,
          });
          if (xhr.onerror) xhr.onerror(new Error("Bridge not ready"));
          if (xhr.onloadend) xhr.onloadend();
          return;
        }
      }

      try {
        // 通过父窗口代理，标记为 XHR 请求
        const response = await proxyViaLocalServer(
          requestUrl,
          requestMethod,
          requestHeaders,
          requestBody,
          false, // isMedia
          true, // isXHR
        );
        const responseType = xhr.responseType || "text";
        let responseValue;
        let responseText;

        if (responseType === "arraybuffer") {
          const buffer = await response.arrayBuffer();
          responseValue = buffer;
          responseText = "";
        } else if (responseType === "blob") {
          const blob = await response.blob();
          responseValue = blob;
          responseText = "";
        } else if (responseType === "json") {
          responseText = await response.text();
          try {
            responseValue = JSON.parse(responseText);
          } catch (e) {
            responseValue = responseText;
          }
        } else {
          responseText = await response.text();
          responseValue = responseText;
        }

        responseHeaders = {};
        response.headers.forEach((value, key) => {
          responseHeaders[key] = value;
        });

        console.log("[PWA Adapt] XHR response:", response.status);

        Object.defineProperty(xhr, "status", {
          value: response.status,
          writable: false,
        });
        Object.defineProperty(xhr, "statusText", {
          value: response.statusText || "OK",
          writable: false,
        });
        Object.defineProperty(xhr, "responseText", {
          value: responseText,
          writable: false,
        });
        Object.defineProperty(xhr, "response", {
          value: responseValue,
          writable: false,
        });
        Object.defineProperty(xhr, "readyState", { value: 4, writable: false });

        if (xhr.onreadystatechange) xhr.onreadystatechange();
        if (xhr.onload) xhr.onload();
        if (xhr.onloadend) xhr.onloadend();
      } catch (err) {
        console.error("[PWA Adapt] XHR proxy error:", err);
        Object.defineProperty(xhr, "status", { value: 0, writable: false });
        Object.defineProperty(xhr, "readyState", { value: 4, writable: false });
        if (xhr.onerror) xhr.onerror(err);
        if (xhr.onloadend) xhr.onloadend();
      }
    };

    return xhr;
  };

  Object.setPrototypeOf(window.XMLHttpRequest, OriginalXHR);
  window.XMLHttpRequest.prototype = OriginalXHR.prototype;
}

export function setupImageProxy(tauriBridge) {
  // 拦截图片加载 - 在 onload 中处理可以确保图片已经设置 src
  function interceptImage(img) {
    if (img.dataset.intercepted) return;
    img.dataset.intercepted = "true";

    // 保存原始 src getter/setter
    const srcDescriptor = Object.getOwnPropertyDescriptor(
      HTMLImageElement.prototype,
      "src",
    );
    const originalSrcSetter = srcDescriptor.set;
    const originalSrcGetter = srcDescriptor.get;

    // 覆盖 src 属性
    Object.defineProperty(img, "src", {
      get() {
        return originalSrcGetter.call(this);
      },
      set(value) {
        const proxiedValue = proxyImageUrl(value);
        originalSrcSetter.call(this, proxiedValue);
      },
      configurable: true,
    });

    // 处理当前已设置的 src
    const currentSrc = originalSrcGetter.call(img);
    if (currentSrc && !img.dataset.proxied) {
      const proxiedUrl = proxyImageUrl(currentSrc);
      if (proxiedUrl !== currentSrc) {
        originalSrcSetter.call(img, proxiedUrl);
      }
    }
  }

  function proxyImageUrl(src) {
    if (!src || typeof src !== "string") return src;

    // 跳过已经代理的、data URL、blob URL
    if (
      src.startsWith("data:") ||
      src.startsWith("blob:") ||
      src.startsWith("http://localhost:19315")
    ) {
      return src;
    }

    // 跳过相对路径和同源 URL
    try {
      const url = new URL(src, window.location.href);
      if (url.origin === window.location.origin && !src.startsWith("http")) {
        return src;
      }
    } catch (e) {
      return src;
    }

    // 返回代理 URL
    return `http://localhost:19315/static/${encodeURIComponent(src)}`;
  }

  function initProxy() {
    // 拦截页面中所有现有的 img 元素
    document.querySelectorAll("img").forEach(interceptImage);

    // 监听新添加的 img 元素
    const observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeName === "IMG") {
            interceptImage(node);
          } else if (node.querySelectorAll) {
            node.querySelectorAll("img").forEach(interceptImage);
          }
        });
      });
    });

    if (document.body) {
      observer.observe(document.body, { childList: true, subtree: true });
    }
  }

  if (document.readyState === "complete") {
    initProxy();
  } else {
    window.addEventListener("load", initProxy);
  }
}

/**
 * 获取媒体代理 URL
 * 将远程音视频 URL 转换为本地代理 URL，可直接用于 <audio> 或 <video> 标签
 * @param {string} url - 原始媒体 URL
 * @param {Object} headers - 可选的自定义 headers（如 Referer, User-Agent 等）
 * @returns {string} 代理 URL
 */
export function getMediaProxyUrl(url, headers = {}) {
  const params = new URLSearchParams();
  params.append("url", url);

  // 添加自定义 headers 到 URL 参数
  Object.entries(headers).forEach(([key, value]) => {
    params.append(`header_${key}`, value);
  });

  return `http://localhost:19315/media/proxy?${params.toString()}`;
}

/**
 * 获取图片代理 URL
 * 将远程图片 URL 转换为本地代理 URL，可直接用于 <img> 标签
 * 使用 /api/proxy 路由（非流式，支持完整的响应处理）
 * @param {string} url - 原始图片 URL
 * @param {Object} headers - 可选的自定义 headers（如 Referer, User-Agent 等）
 * @returns {string} 代理 URL
 */
export function getImageProxyUrl(url, headers = {}) {
  const params = new URLSearchParams();
  params.append("url", url);

  // 添加自定义 headers 到 URL 参数
  Object.entries(headers).forEach(([key, value]) => {
    params.append(`header_${key}`, value);
  });

  return `http://localhost:19315/api/proxy?${params.toString()}`;
}

/**
 * 获取本地文件 URL
 * 将本地文件路径转换为可通过 HTTP 访问的 URL
 * @param {string} filePath - 本地文件路径
 * @returns {string} HTTP URL
 */
export function getLocalFileUrl(filePath) {
  const encodedPath = encodeURIComponent(filePath);
  return `http://localhost:19315/local/file/${encodedPath}`;
}
