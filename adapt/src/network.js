/**
 * Network APIs (fetch/XHR proxy)
 */

import { originalFetch, OriginalXHR } from "./core.js";

export function createNetwork(bridge) {
  return {
    async fetch(url, options = {}) {
      const urlStr = url.toString();

      // 跳过 static:// 协议的请求（让浏览器直接使用原生协议）
      if (
        urlStr.startsWith("static://") ||
        urlStr.startsWith("http://static.localhost")
      ) {
        return originalFetch(url, options);
      }

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

      try {
        // 检测 responseType - 从 options、headers 或 URL 扩展名中获取
        let respType = options.responseType;

        // 检查 URL 扩展名判断是否为二进制资源
        if (!respType) {
          const urlLower = urlStr.toLowerCase();
          if (urlLower.match(/\.(jpg|jpeg|png|gif|webp|bmp|ico|svg)(\?|$)/)) {
            respType = "blob";
          } else if (urlLower.match(/\.(mp3|wav|ogg|flac|aac|m4a)(\?|$)/)) {
            respType = "blob";
          } else if (urlLower.match(/\.(mp4|webm|avi|mov|mkv)(\?|$)/)) {
            respType = "blob";
          } else if (urlLower.match(/\.(pdf|zip|rar|7z|tar|gz)(\?|$)/)) {
            respType = "blob";
          }
        }

        // 检查 Accept header
        if (!respType && options.headers) {
          const acceptHeader =
            options.headers["Accept"] || options.headers["accept"];
          if (acceptHeader) {
            if (
              acceptHeader.includes("image/") ||
              acceptHeader.includes("audio/") ||
              acceptHeader.includes("video/") ||
              acceptHeader.includes("application/octet-stream")
            ) {
              respType = "blob";
            }
          }
        }
        respType = respType || "text";

        const result = await bridge.invoke("proxy_fetch", {
          url: urlStr,
          method: options.method || "GET",
          headers: options.headers || {},
          body: options.body || null,
          responseType: respType,
        });

        const responseData = result.data || result;

        // 创建新的 headers 对象
        const responseHeaders = new Headers(responseData.headers || {});

        let body = responseData.body;

        // 处理二进制数据（base64 解码）
        if (responseData.is_base64 || respType === "arraybuffer" || respType === "blob") {
          const byteCharacters = atob(body);
          const byteArray = new Uint8Array(byteCharacters.length);
          for (let i = 0; i < byteCharacters.length; i++) {
            byteArray[i] = byteCharacters.charCodeAt(i);
          }
          body = byteArray.buffer;
        }

        // 创建 Response，确保 body 是可读的
        if (body instanceof ArrayBuffer) {
          // 对于二进制数据，创建新的 Uint8Array 视图
          return new Response(new Uint8Array(body), {
            status: responseData.status,
            headers: responseHeaders,
          });
        }

        return new Response(body, {
          status: responseData.status,
          headers: responseHeaders,
        });
      } catch (invokeError) {
        console.error("[PWA Adapt] Proxy fetch failed:", invokeError);
        return new Response(JSON.stringify({ error: invokeError.message }), {
          status: 500,
          statusText: "Bad Gateway",
          headers: { "Content-Type": "application/json" },
        });
      }
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
          console.log("[PWA Adapt] XHR proxy:", requestUrl);

          if (!tauriBridge._ready) {
            let attempts = 0;
            while (!tauriBridge._ready && attempts < 50) {
              await new Promise((resolve) => setTimeout(resolve, 100));
              attempts++;
            }
          }

          try {
            // 根据 XHR 的 responseType 设置 proxy_fetch 的 response_type
            const respType = xhr.responseType;
            const proxyResponseType =
              respType === "arraybuffer" || respType === "blob"
                ? respType
                : "text";

            const result = await tauriBridge.invoke("proxy_fetch", {
              url: requestUrl,
              method: requestMethod,
              headers: requestHeaders,
              body: requestBody,
              responseType: proxyResponseType,
            });

            const responseData = result.data || result;
            responseHeaders = responseHeaders || {};

            let responseValue = responseData.body;
            const responseType = xhr.responseType || "text";

            if (responseType === "json") {
              try {
                responseValue = JSON.parse(responseData.body);
              } catch (e) {
                responseValue = responseData.body;
              }
            } else if (
              responseType === "arraybuffer" ||
              responseType === "blob"
            ) {
              // 将 base64 转换为 ArrayBuffer
              const base64 = responseData.body;
              const binary = atob(base64);
              const bytes = new Uint8Array(binary.length);
              for (let i = 0; i < binary.length; i++) {
                bytes[i] = binary.charCodeAt(i);
              }
              if (responseType === "arraybuffer") {
                responseValue = bytes.buffer;
              } else {
                responseValue = new Blob([bytes], {
                  type:
                    responseData.headers["content-type"] ||
                    "application/octet-stream",
                });
              }
            }

            console.log(
              "[PWA Adapt] Proxy response:",
              responseData.status,
              responseData.headers,
              responseValue,
            );
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
  function proxyImage(img) {
    const src = img.getAttribute("src") || img.src;
    if (!src || img.dataset.proxied) return;

    console.log("[PWA Adapt] Image src:", src);

    // 跳过 blob 和 data URL
    if (src.startsWith("blob:") || src.startsWith("data:")) return;

    // 跳过已经代理的
    if (
      src.startsWith("static://") ||
      src.startsWith("http://static.localhost")
    )
      return;

    try {
      const url = new URL(src, window.location.href);
      // 跳过同源的
      if (url.origin === window.location.origin && !src.startsWith("http"))
        return;
    } catch (e) {
      // URL 解析失败，可能是相对路径，不处理
      return;
    }

    img.dataset.proxied = "true";
    img.dataset.originalSrc = src;

    const staticUrl = `${tauriBridge.staticBaseUrl}/${src}`;
    console.log("[PWA Adapt] Proxy image:", src, "->", staticUrl);
    img.src = staticUrl;
  }

  function initProxy() {
    console.log("[PWA Adapt] Initializing image proxy...");

    // 处理已存在的图片
    document.querySelectorAll("img").forEach(proxyImage);

    // 监听动态添加的图片
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

    if (document.body) {
      observer.observe(document.body, { childList: true, subtree: true });
    }

    // 延迟再执行一次，确保动态加载的图片也被处理
    setTimeout(() => {
      document.querySelectorAll("img").forEach(proxyImage);
    }, 1000);
  }

  // 页面完全加载后执行（包括所有资源）
  if (document.readyState === "complete") {
    initProxy();
  } else {
    window.addEventListener("load", initProxy);
  }
}
