/**
 * Network APIs (fetch/XHR proxy)
 */

import { originalFetch, OriginalXHR } from "./core.js";

export function createNetwork(bridge) {
  return {
    async fetch(input, init = {}) {
      // 处理 input 可能是 Request 对象的情况
      let urlStr;
      let options = { ...init };
      
      if (input instanceof Request) {
        urlStr = input.url;
        // 从 Request 对象中提取选项
        options.method = init.method || input.method;
        options.headers = init.headers || {};
        // 复制 Request 的 headers
        input.headers.forEach((value, key) => {
          if (!options.headers[key]) {
            options.headers[key] = value;
          }
        });
      } else {
        urlStr = input.toString();
      }

      // 跳过 static:// 协议的请求（让浏览器直接使用原生协议）
      if (
        urlStr.startsWith("static://") ||
        urlStr.startsWith("http://static.localhost")
      ) {
        return originalFetch(urlStr, options);
      }

      // 检测是否是流式请求 (AI SSE/Stream)
      const isStreamRequest = options.headers?.['Accept'] === 'text/event-stream' ||
                              options.headers?.['accept'] === 'text/event-stream' ||
                              (typeof options.body === 'string' && options.body.includes('"stream":true')) ||
                              (typeof options.body === 'string' && options.body.includes('"stream": true'));
      
      // 检测是否标记为直接请求（不走代理，用于支持 CORS 的 API）
      const isDirectRequest = options.headers?.['X-Direct-Request'] === 'true' ||
                              options.headers?.['x-direct-request'] === 'true';
      
      // 使用 fetch:// 协议直接代理请求（比 invoke 快）
      if (urlStr.startsWith("http://") || urlStr.startsWith("https://")) {
        // 如果是标记为直接请求，直接走原生 fetch
        if (isDirectRequest) {
          console.log("[PWA Adapt] Direct request (no proxy):", urlStr);
          // 删除标记后再发送，避免发送到服务器
          const cleanOptions = {...options};
          if (cleanOptions.headers) {
            delete cleanOptions.headers['X-Direct-Request'];
            delete cleanOptions.headers['x-direct-request'];
          }
          return await originalFetch(urlStr, cleanOptions);
        }
        
        // 如果是流式请求，需要特殊处理
        if (isStreamRequest) {
          console.log("[PWA Adapt] Stream request detected, using streaming proxy:", urlStr);
          // 流式请求暂时回退到原生 fetch（可能需要处理 CORS）
          // 或者可以实现专门的流式代理命令
          try {
            return await originalFetch(urlStr, options);
          } catch (e) {
            console.warn("[PWA Adapt] Direct stream fetch failed, trying proxy:", e);
          }
        }
        
        // Android 上映射为 http://fetch.localhost/，其他平台用 fetch://localhost/
        const isAndroid = /Android/i.test(navigator.userAgent);
        const fetchUrl = isAndroid 
          ? 'http://fetch.localhost/proxy'
          : 'fetch://localhost/proxy';
        
        try {
          // 构造 headers，自动添加 Referer（当前页面 URL）
          const headers = { ...options.headers };
          if (!headers['Referer'] && !headers['referer']) {
            headers['Referer'] = location.href;
          }
          
          // 构造请求 body
          const proxyBody = JSON.stringify({
            target: urlStr,
            method: options.method || 'GET',
            headers: headers,
            body: options.body
          });
          
          // 发送给 fetch://localhost/proxy
          return await originalFetch(fetchUrl, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json'
            },
            body: proxyBody
          });
        } catch (error) {
          console.error("[PWA Adapt] Fetch protocol error:", error);
          console.log("[PWA Adapt] Falling back to proxy_fetch");
        }
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

        // 创建 Response - 确保 body 是 Uint8Array 以支持 blob()
        let responseBody;
        if (body instanceof ArrayBuffer) {
          responseBody = new Uint8Array(body);
        } else {
          responseBody = body;
        }

        return new Response(responseBody, {
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
            // 使用 fetch:// 协议（比 proxy_fetch 快）
            const isAndroid = /Android/i.test(navigator.userAgent);
            const fetchUrl = isAndroid 
              ? 'http://fetch.localhost/proxy'
              : 'fetch://localhost/proxy';
            
            // 构造请求 body
            const proxyBody = JSON.stringify({
              target: requestUrl,
              method: requestMethod,
              headers: requestHeaders,
              body: requestBody
            });
            
            // 使用原生 fetch 发起请求
            const response = await originalFetch(fetchUrl, {
              method: 'POST',
              headers: {
                'Content-Type': 'application/json'
              },
              body: proxyBody
            });
            
            // 读取响应
            const responseType = xhr.responseType || "text";
            let responseValue;
            let responseText;
            
            if (responseType === "arraybuffer") {
              responseValue = await response.arrayBuffer();
              responseText = "";
            } else if (responseType === "blob") {
              responseValue = await response.blob();
              responseText = "";
            } else if (responseType === "json") {
              responseText = await response.text();
              try {
                responseValue = JSON.parse(responseText);
              } catch (e) {
                responseValue = responseText;
              }
            } else {
              // text 或其他
              responseText = await response.text();
              responseValue = responseText;
            }
            
            // 解析响应头
            responseHeaders = {};
            response.headers.forEach((value, key) => {
              responseHeaders[key] = value;
            });

            console.log(
              "[PWA Adapt] XHR proxy response:",
              response.status,
              response.statusText,
            );
            
            Object.defineProperty(xhr, "status", {
              value: response.status,
              writable: false,
            });
            Object.defineProperty(xhr, "statusText", {
              value: response.statusText || (response.status === 200 ? "OK" : ""),
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
