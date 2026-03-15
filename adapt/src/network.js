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
function proxyViaLocalServer(url, method, headers, body, isMedia = false) {
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
          const blob = new Blob([responseData.body]);
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

    // 发送代理请求给父窗口，使用 isMedia 标记来选择路由
    window.parent.postMessage(
      {
        type: "ADAPT_PROXY_REQUEST",
        requestId,
        url,
        method,
        headers,
        body,
        isMedia, // 标记是否为媒体请求
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
        headers["User-Agent"] =
          "Mozilla/5.0 (Linux; Android 13; TECNO BG6 Build/TP1A.220624.014) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.7632.159 Mobile Safari/537.36";

        // 添加简单的 Accept 头（与 curl 一致）
        if (!headers["Accept"] && !headers["accept"]) {
          headers["Accept"] = "*/*";
        }

        return await proxyViaLocalServer(
          urlStr,
          options.method || "GET",
          headers,
          options.body,
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
      return originalOpen(method, url, async, user, password);
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

      try {
        const urlObj = new URL(requestUrl, window.location.href);
        if (urlObj.origin !== window.location.origin) {
          if (!tauriBridge._ready) {
            let attempts = 0;
            while (!tauriBridge._ready && attempts < 50) {
              await new Promise((resolve) => setTimeout(resolve, 100));
              attempts++;
            }
          }

          try {
            // 通过父窗口代理
            const response = await proxyViaLocalServer(
              requestUrl,
              requestMethod,
              requestHeaders,
              requestBody,
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
            Object.defineProperty(xhr, "readyState", {
              value: 4,
              writable: false,
            });

            if (xhr.onreadystatechange) xhr.onreadystatechange();
            if (xhr.onload) xhr.onload();
            if (xhr.onloadend) xhr.onloadend();

            return;
          } catch (err) {
            Object.defineProperty(xhr, "status", {
              value: 500,
              writable: false,
            });
            Object.defineProperty(xhr, "readyState", {
              value: 4,
              writable: false,
            });
            if (xhr.onerror) xhr.onerror(err);
            if (xhr.onloadend) xhr.onloadend();
            return;
          }
        }
      } catch (e) {
        console.error(e);
      }

      return originalSend(body);
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
 * @returns {string} 代理 URL
 */
export function getMediaProxyUrl(url) {
  const encodedUrl = encodeURIComponent(url);
  return `http://localhost:${LOCAL_SERVER_PORT}/media/proxy?url=${encodedUrl}`;
}

/**
 * 获取本地文件 URL
 * 将本地文件路径转换为可通过 HTTP 访问的 URL
 * @param {string} filePath - 本地文件路径
 * @returns {string} HTTP URL
 */
export function getLocalFileUrl(filePath) {
  const encodedPath = encodeURIComponent(filePath);
  return `http://localhost:${LOCAL_SERVER_PORT}/local/file/${encodedPath}`;
}
