/**
 * Network APIs (fetch/XHR proxy)
 */

import { originalFetch, OriginalXHR } from "./core.js";

export function createNetwork(bridge) {
  return {
    async fetch(url, options = {}) {
      const urlStr = url.toString();

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
        const result = await bridge.invoke("proxy_fetch", {
          url: urlStr,
          method: options.method || "GET",
          headers: options.headers || {},
          body: options.body || null,
          responseType: options.responseType || "text",
        });

        const responseData = result.data || result;

        let body = responseData.body;
        const respType = options.responseType || "text";

        if (
          responseData.is_base64 ||
          respType === "arraybuffer" ||
          respType === "blob"
        ) {
          const byteCharacters = atob(body);
          const byteArray = new Uint8Array(byteCharacters.length);
          for (let i = 0; i < byteCharacters.length; i++) {
            byteArray[i] = byteCharacters.charCodeAt(i);
          }

          if (respType === "arraybuffer") {
            body = byteArray.buffer;
          } else {
            body = new Blob([byteArray], {
              type:
                responseData.headers["content-type"] ||
                "application/octet-stream",
            });
          }
        }

        return new Response(body, {
          status: responseData.status,
          headers: responseData.headers,
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
            const result = await tauriBridge.invoke("proxy_fetch", {
              url: requestUrl,
              method: requestMethod,
              headers: requestHeaders,
              body: requestBody,
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
            }

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
      } catch (e) {}

      return originalSend(body);
    };

    return xhr;
  };

  Object.setPrototypeOf(window.XMLHttpRequest, OriginalXHR);
  window.XMLHttpRequest.prototype = OriginalXHR.prototype;
}

export function setupImageProxy(tauriBridge) {
  function proxyImage(img) {
    const src = img.getAttribute('src') || img.src;
    if (!src || img.dataset.proxied) return;

    console.log('[PWA Adapt] Image src:', src);
    
    // 跳过 blob 和 data URL
    if (src.startsWith("blob:") || src.startsWith("data:")) return;
    
    // 跳过已经代理的
    if (src.startsWith("static://") || src.startsWith("http://static.localhost")) return;
    
    try {
      const url = new URL(src, window.location.href);
      // 跳过同源的
      if (url.origin === window.location.origin && !src.startsWith('http')) return;
    } catch (e) {
      // URL 解析失败，可能是相对路径，不处理
      return;
    }

    img.dataset.proxied = "true";
    img.dataset.originalSrc = src;

    const staticUrl = `${tauriBridge.staticBaseUrl}/${src}`;
    console.log('[PWA Adapt] Proxy image:', src, '->', staticUrl);
    img.src = staticUrl;
  }

  function initProxy() {
    console.log('[PWA Adapt] Initializing image proxy...');
    
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
  if (document.readyState === 'complete') {
    initProxy();
  } else {
    window.addEventListener('load', initProxy);
  }
}

