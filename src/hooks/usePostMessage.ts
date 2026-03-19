import { useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fetch as tauriFetch } from "@tauri-apps/plugin-http";
import { getCookies, setCookies } from "../cookie";
import type { AppInfo } from "../components/types";

interface UsePostMessageProps {
  apps: AppInfo[];
  showMessage: (type: "success" | "error", text: string) => void;
}

// 数据库连接缓存（每个 pwaId 一个连接）
const dbCache = new Map<string, any>();
const dbLoadingPromise = new Map<string, Promise<any>>();

async function getDbConnection(pwaId: string): Promise<any> {
  // 检查缓存中是否已有连接
  if (dbCache.has(pwaId)) {
    return dbCache.get(pwaId);
  }

  // 检查是否正在加载中
  if (dbLoadingPromise.has(pwaId)) {
    return dbLoadingPromise.get(pwaId);
  }

  const loadPromise = (async () => {
    try {
      const { default: Database } = await import("@tauri-apps/plugin-sql");
      const { appDataDir } = await import("@tauri-apps/api/path");
      const { mkdir } = await import("@tauri-apps/plugin-fs");
      const appDataPath = await appDataDir();

      const dbDir = `${appDataPath}/pwa_data/${pwaId}`;
      try {
        await mkdir(dbDir, { recursive: true });
      } catch (e) {
        // 目录已存在
      }

      const dbPath = `sqlite:${dbDir}/${pwaId}.db`;
      const db = await Database.load(dbPath);
      dbCache.set(pwaId, db);
      return db;
    } finally {
      dbLoadingPromise.delete(pwaId);
    }
  })();

  dbLoadingPromise.set(pwaId, loadPromise);
  return loadPromise;
}

// 清理指定 PWA 的数据库连接
export async function closePwaDbConnection(pwaId: string) {
  const db = dbCache.get(pwaId);
  if (db) {
    try {
      await db.close();
    } catch (e) {
      console.error(`[DB] Failed to close connection for ${pwaId}:`, e);
    }
    dbCache.delete(pwaId);
  }
}

// 清理所有 PWA 数据库连接
export async function closeAllPwaDbConnections() {
  for (const [pwaId, db] of dbCache.entries()) {
    try {
      await db.close();
    } catch (e) {
      console.error(`[DB] Failed to close connection for ${pwaId}:`, e);
    }
  }
  dbCache.clear();
  dbLoadingPromise.clear();
}

export function usePostMessage({ apps, showMessage }: UsePostMessageProps) {
  const iframesRef = useRef<Record<string, HTMLIFrameElement>>({});
  const processedRequests = useRef<Set<string>>(new Set());

  const handleMessage = useCallback(
    async (event: MessageEvent) => {
      // ADAPT_READY - PWA 初始化完成
      if (event.data?.type === "ADAPT_READY") {
        const entry = Object.entries(iframesRef.current).find(
          ([_, f]) => f.contentWindow === event.source
        );
        const appId = entry ? entry[0] : null;

        event.source?.postMessage(
          { type: "ADAPT_PARENT_READY", pwaId: appId },
          "*"
        );
        return;
      }

      // ADAPT_GET_APP_INFO - PWA 请求获取 app 信息
      if (event.data?.type === "ADAPT_GET_APP_INFO") {
        const { requestId } = event.data;
        const entry = Object.entries(iframesRef.current).find(
          ([_, f]) => f.contentWindow === event.source
        );
        if (entry) {
          const [appId] = entry;
          event.source?.postMessage(
            {
              type: "ADAPT_APP_INFO_RESPONSE",
              requestId,
              appId,
              appInfo: apps.find((a) => a.id === appId),
            },
            "*"
          );
        } else {
          event.source?.postMessage(
            {
              type: "ADAPT_APP_INFO_RESPONSE",
              requestId,
              error: "Unauthorized: not a registered PWA",
            },
            "*"
          );
        }
        return;
      }

      // BROWSER_SYNC_COOKIES - 浏览器模式同步 Cookies
      if (event.data?.type === "BROWSER_SYNC_COOKIES") {
        const { domain, cookies } = event.data;
        if (domain && cookies !== undefined) {
          try {
            await invoke("sync_webview_cookies", {
              domain,
              cookies,
              userAgent: navigator.userAgent,
            });
            showMessage("success", `已同步 ${domain} 的 Cookies`);
          } catch (error) {
            showMessage("error", `同步 Cookies 失败: ${String(error)}`);
          }
        }
        return;
      }

      // ADAPT_PROXY_REQUEST - HTTP 代理请求
      if (event.data?.type === "ADAPT_PROXY_REQUEST") {
        await handleProxyRequest(event, showMessage);
        return;
      }

      // 以下请求需要验证 iframe 来源
      const iframe = Object.values(iframesRef.current).find(
        (f) => f.contentWindow === event.source
      );
      if (!iframe) return;

      const entry = Object.entries(iframesRef.current).find(
        ([_, f]) => f === iframe
      );
      const verifiedAppId = entry ? entry[0] : null;
      if (!verifiedAppId) return;

      // ADAPT_SQL_REQUEST - SQL 查询
      if (event.data?.type === "ADAPT_SQL_REQUEST") {
        await handleSqlRequest(event, verifiedAppId);
        return;
      }

      // ADAPT_STORE_REQUEST - Store 操作
      if (event.data?.type === "ADAPT_STORE_REQUEST") {
        await handleStoreRequest(event, verifiedAppId);
        return;
      }

      // ADAPT_CACHE_REQUEST - Cache 操作
      if (event.data?.type === "ADAPT_CACHE_REQUEST") {
        await handleCacheRequest(event, verifiedAppId);
        return;
      }

      // ADAPT_INVOKE - 调用 Tauri 命令
      if (event.data?.type === "ADAPT_INVOKE") {
        await handleInvoke(event, verifiedAppId);
        return;
      }
    },
    [apps, showMessage]
  );

  useEffect(() => {
    window.addEventListener("message", handleMessage);
    
    // 页面卸载前关闭连接（手机切后台或杀进程时触发）
    const handleBeforeUnload = () => {
      closeAllPwaDbConnections();
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    
    // Android 返回键或应用切换
    document.addEventListener("pause", handleBeforeUnload, false);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        closeAllPwaDbConnections();
      }
    });
    
    return () => {
      window.removeEventListener("message", handleMessage);
      window.removeEventListener("beforeunload", handleBeforeUnload);
      document.removeEventListener("pause", handleBeforeUnload, false);
      closeAllPwaDbConnections();
    };
  }, [handleMessage]);

  return { iframesRef };
}

// 处理代理请求
async function handleProxyRequest(
  event: MessageEvent,
  showMessage: (type: "success" | "error", text: string) => void
) {
  const { requestId, url, method, headers, body } = event.data;
  
  // 去重检查
  if (processedRequestIds.has(requestId)) {
    console.log(`[Proxy] Duplicate request ${requestId}, skipping`);
    return;
  }
  processedRequestIds.add(requestId);
  
  let requestBody = body;

  try {
    requestBody = JSON.parse(requestBody);
  } catch (e) {}

  try {
    // 本地文件直接走本地服务器
    if (url.startsWith("http://localhost:19315")) {
      const response = await fetch(url);
      const data = await formatResponse(response);
      event.source?.postMessage(
        { type: "ADAPT_PROXY_RESPONSE", requestId, success: true, data },
        "*"
      );
      return;
    }

    // 远程请求使用 tauri-plugin-http
    const fetchOptions: RequestInit = {
      method: method || "GET",
      headers: { ...(headers || {}) },
    };

    // 添加 cookies
    try {
      const urlObj = new URL(url);
      const domain = urlObj.hostname;
      const [browserCookies, webviewCookies] = await Promise.all([
        getCookies(url, "browser"),
        getCookies(url, "webview"),
      ]);

      const allCookies: string[] = [];
      for (const cookies of [browserCookies, webviewCookies]) {
        for (const [name, value] of Object.entries(cookies)) {
          allCookies.push(`${name}=${value}`);
        }
      }

      if (allCookies.length > 0) {
        (fetchOptions.headers as Record<string, string>)["Cookie"] =
          allCookies.join("; ");
      }
    } catch (e) {
      console.error("[Proxy] Failed to get cookies:", e);
    }

    if (requestBody) {
      fetchOptions.body = requestBody;
    }

    const response = await tauriFetch(url, fetchOptions);

    // 保存 Set-Cookie
    try {
      const setCookieHeaders = response.headers.getSetCookie?.() || [];
      if (setCookieHeaders.length > 0) {
        const cookiesToSave = setCookieHeaders
          .map((cookieStr: string) => {
            const eqPos = cookieStr.indexOf("=");
            if (eqPos > 0) {
              const key = cookieStr.substring(0, eqPos).trim();
              const value = cookieStr.substring(eqPos + 1).split(";")[0].trim();
              return `${key}=${value}`;
            }
            return null;
          })
          .filter(Boolean) as string[];

        if (cookiesToSave.length > 0) {
          await setCookies(url, "browser", cookiesToSave);
        }
      }
    } catch (e) {
      console.error("[Proxy] Failed to save cookies:", e);
    }

    const data = await formatResponse(response);
    event.source?.postMessage(
      { type: "ADAPT_PROXY_RESPONSE", requestId, success: true, data },
      "*"
    );
  } catch (error) {
    console.error("[Proxy] Request failed:", error);
    event.source?.postMessage(
      {
        type: "ADAPT_PROXY_RESPONSE",
        requestId,
        success: false,
        error: String(error),
      },
      "*"
    );
  }
}

// 格式化响应
async function formatResponse(response: Response) {
  const contentType = response.headers.get("content-type") || "";
  const isBinary =
    contentType.startsWith("image/") ||
    contentType.startsWith("audio/") ||
    contentType.startsWith("video/") ||
    contentType === "application/octet-stream";

  let body: string;
  if (isBinary) {
    const arrayBuffer = await response.arrayBuffer();
    const bytes = new Uint8Array(arrayBuffer);
    let binary = "";
    for (let i = 0; i < bytes.byteLength; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    body = btoa(binary);
  } else {
    body = await response.text();
  }

  return {
    status: response.status,
    statusText: response.statusText,
    headers: Object.fromEntries(response.headers.entries()),
    body,
    isBase64: isBinary,
  };
}

// 已处理的请求 ID 集合（防止重复处理）
const processedRequestIds = new Set<string>();

// 处理 SQL 请求
async function handleSqlRequest(event: MessageEvent, appId: string) {
  const { requestId, sql, params } = event.data;
  
  // 去重检查
  if (processedRequestIds.has(requestId)) {
    console.log(`[SQL] Duplicate request ${requestId}, skipping`);
    return;
  }
  processedRequestIds.add(requestId);
  
  // 清理旧请求 ID（防止内存泄漏）
  if (processedRequestIds.size > 1000) {
    const oldestEntries = Array.from(processedRequestIds).slice(0, 500);
    oldestEntries.forEach(id => processedRequestIds.delete(id));
  }
  
  try {
    const db = await getDbConnection(appId);
    let result;
    if (sql.trim().toLowerCase().startsWith("select")) {
      result = await db.select(sql, params);
    } else {
      await db.execute(sql, params);
      result = { success: true };
    }
    event.source?.postMessage(
      { type: "ADAPT_SQL_RESPONSE", requestId, success: true, data: result },
      "*"
    );
  } catch (error) {
    console.error("[SQL] Error:", error);
    // 如果是连接池错误，清理缓存让下次重新连接
    const errorStr = String(error);
    if (errorStr.includes("closed pool") || errorStr.includes("Connection")) {
      console.log(`[DB] Cleaning cache for ${appId} due to connection error`);
      dbCache.delete(appId);
    }
    event.source?.postMessage(
      {
        type: "ADAPT_SQL_RESPONSE",
        requestId,
        success: false,
        error: errorStr,
      },
      "*"
    );
  }
}

// 处理 Store 请求
async function handleStoreRequest(event: MessageEvent, appId: string) {
  const { requestId, action, key, value } = event.data;
  
  // 去重检查
  if (processedRequestIds.has(requestId)) {
    console.log(`[Store] Duplicate request ${requestId}, skipping`);
    return;
  }
  processedRequestIds.add(requestId);
  
  try {
    const { appDataDir } = await import("@tauri-apps/api/path");
    const { mkdir } = await import("@tauri-apps/plugin-fs");
    const appDataPath = await appDataDir();
    const storesDir = `${appDataPath}/pwa_data/stores`;
    await mkdir(storesDir, { recursive: true });

    const { load } = await import("@tauri-apps/plugin-store");
    const store = await load(`pwa_data/stores/pwa-${appId}.json`, {
      autoSave: true,
    });

    let result;
    switch (action) {
      case "get":
        result = await store.get(key);
        break;
      case "set":
        await store.set(key, value);
        await store.save();
        result = true;
        break;
      case "delete":
        await store.delete(key);
        await store.save();
        result = true;
        break;
      case "clear":
        await store.clear();
        await store.save();
        result = true;
        break;
      case "entries":
        result = await store.entries();
        break;
      default:
        throw new Error(`Unknown store action: ${action}`);
    }

    event.source?.postMessage(
      { type: "ADAPT_STORE_RESPONSE", requestId, success: true, data: result },
      "*"
    );
  } catch (error) {
    console.error("[Store] Error:", error);
    event.source?.postMessage(
      {
        type: "ADAPT_STORE_RESPONSE",
        requestId,
        success: false,
        error: String(error),
      },
      "*"
    );
  }
}

// 处理 Cache 请求
async function handleCacheRequest(event: MessageEvent, appId: string) {
  const { requestId, action, namespace, key, value } = event.data;
  
  // 去重检查
  if (processedRequestIds.has(requestId)) {
    console.log(`[Cache] Duplicate request ${requestId}, skipping`);
    return;
  }
  processedRequestIds.add(requestId);
  
  try {
    const { appDataDir } = await import("@tauri-apps/api/path");
    const { mkdir } = await import("@tauri-apps/plugin-fs");
    const appDataPath = await appDataDir();
    const cacheDir = `${appDataPath}/pwa_data/cache`;
    await mkdir(cacheDir, { recursive: true });

    const { load } = await import("@tauri-apps/plugin-store");
    const store = await load(
      `pwa_data/cache/pwa-${appId}-cache-${namespace}.json`,
      { autoSave: false }
    );

    let result;
    switch (action) {
      case "get":
        result = await store.get(key);
        break;
      case "set":
        await store.set(key, value);
        await store.save();
        result = true;
        break;
      case "delete":
        await store.delete(key);
        await store.save();
        result = true;
        break;
      case "clear":
        await store.clear();
        await store.save();
        result = true;
        break;
      default:
        throw new Error(`Unknown cache action: ${action}`);
    }

    event.source?.postMessage(
      { type: "ADAPT_CACHE_RESPONSE", requestId, success: true, data: result },
      "*"
    );
  } catch (error) {
    console.error("[Cache] Error:", error);
    event.source?.postMessage(
      {
        type: "ADAPT_CACHE_RESPONSE",
        requestId,
        success: false,
        error: String(error),
      },
      "*"
    );
  }
}

// 处理 Invoke 请求
async function handleInvoke(event: MessageEvent, appId: string) {
  const { cmd, payload, requestId } = event.data;
  try {
    let finalPayload = payload;
    if (cmd.startsWith("sqlite_")) {
      finalPayload = { ...payload, pwaId: appId };
    } else if (cmd.startsWith("kv_")) {
      finalPayload = { ...payload, appId };
    }

    const result = await invoke(cmd, finalPayload);
    event.source?.postMessage(
      {
        type: "ADAPT_RESULT",
        cmd,
        requestId,
        result: JSON.parse(JSON.stringify(result)),
      },
      "*"
    );
  } catch (error) {
    event.source?.postMessage(
      {
        type: "ADAPT_RESULT",
        cmd,
        requestId,
        error: String(error),
      },
      "*"
    );
  }
}
