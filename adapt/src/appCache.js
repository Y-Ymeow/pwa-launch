/**
 * App Cache - 应用本体资源手动缓存
 * 
 * 简化方案：
 * 1. 不劫持任何资源加载
 * 2. 提供手动 precache API，PWA 启动时调用
 * 3. 通过 persistentCache 存储，完全独立于 WebView 缓存
 * 
 * 使用方式：
 * 
 * // PWA 启动时预缓存关键资源
 * await __TAURI__.appCache.precache([
 *   '/app.js',
 *   '/style.css', 
 *   '/manifest.json',
 *   '/icon.png'
 * ]);
 */

// 获取当前应用 ID
function getAppId() {
  try {
    const contextCookie = document.cookie
      .split(";")
      .find((c) => c.trim().startsWith("pwa_context="));
    if (contextCookie) {
      const ctx = contextCookie.trim().substring("pwa_context=".length);
      return ctx.replace(/^\//, '').replace(/\//g, '-');
    }
  } catch (e) {}
  const match = window.location.href.match(/\/(https|http)\/(.+)/);
  if (match) {
    return match[2].replace(/\//g, '-');
  }
  return window.location.hostname || "default";
}

// 获取应用基础 URL
function getAppBaseUrl() {
  const match = window.location.href.match(/\/(https|http)\/(.+?)(?:\/|$)/);
  if (match) {
    return `${match[1]}://${match[2]}`;
  }
  return window.location.origin;
}

/**
 * 预缓存资源列表
 * @param {string[]} urls - 相对路径或绝对 URL 列表
 * @param {Object} options - 选项
 * @param {Function} options.onProgress - 进度回调 (loaded, total)
 * @returns {Promise<{success: number, failed: number}>}
 * 
 * @example
 * await __TAURI__.appCache.precache([
 *   '/app.js',
 *   '/style.css',
 *   '/icons/logo.png'
 * ]);
 */
async function precache(urls, options = {}) {
  const appId = getAppId();
  const baseUrl = getAppBaseUrl();
  const results = { success: 0, failed: 0 };
  const total = urls.length;

  console.log(`[AppCache] Starting precache for ${appId}, ${total} resources`);

  for (let i = 0; i < urls.length; i++) {
    const url = urls[i];
    
    try {
      // 转换为绝对 URL
      const absoluteUrl = url.startsWith('http') ? url : `${baseUrl}${url}`;
      
      // 生成缓存 key
      const cacheKey = `app:${url}`;
      
      // 检查是否已缓存
      const existing = await window.__TAURI__?.persistentCache?.getItem?.(cacheKey);
      if (existing) {
        console.log(`[AppCache] Already cached: ${url}`);
        results.success++;
        continue;
      }

      // 获取资源
      console.log(`[AppCache] Fetching: ${url}`);
      const response = await fetch(absoluteUrl);
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      // 读取为 blob
      const blob = await response.blob();
      
      // 缓存到 persistentCache
      await window.__TAURI__?.persistentCache?.setItem?.(cacheKey, blob, {
        mimeType: blob.type || 'application/octet-stream'
      });

      console.log(`[AppCache] Cached: ${url} (${blob.size} bytes)`);
      results.success++;

      // 进度回调
      if (options.onProgress) {
        options.onProgress(i + 1, total);
      }

    } catch (e) {
      console.error(`[AppCache] Failed to cache ${url}:`, e);
      results.failed++;
    }
  }

  console.log(`[AppCache] Precache complete: ${results.success} success, ${results.failed} failed`);
  return results;
}

/**
 * 从缓存获取资源
 * @param {string} url - 资源路径
 * @returns {Promise<Blob|null>}
 */
async function getFromCache(url) {
  const cacheKey = `app:${url}`;
  
  try {
    const cached = await window.__TAURI__?.persistentCache?.getBlob?.(cacheKey);
    if (cached) {
      console.log(`[AppCache] Cache hit: ${url}`);
      return cached;
    }
  } catch (e) {
    console.error(`[AppCache] Failed to get from cache: ${url}`, e);
  }
  
  return null;
}

/**
 * 检查资源是否在缓存中
 * @param {string} url - 资源路径
 * @returns {Promise<boolean>}
 */
async function isCached(url) {
  const cacheKey = `app:${url}`;
  return await window.__TAURI__?.persistentCache?.exists?.(cacheKey) || false;
}

/**
 * 清除应用缓存
 * @returns {Promise<boolean>}
 */
async function clear() {
  const appId = getAppId();
  
  try {
    // 获取所有缓存
    const allCaches = await window.__TAURI__?.persistentCache?.list?.();
    
    // 删除以 app: 开头的缓存
    const appCaches = allCaches?.filter(item => item.key.startsWith('app:')) || [];
    
    for (const cache of appCaches) {
      await window.__TAURI__?.persistentCache?.removeItem?.(cache.key);
    }
    
    console.log(`[AppCache] Cleared ${appCaches.length} cached resources`);
    return true;
  } catch (e) {
    console.error('[AppCache] Failed to clear cache:', e);
    return false;
  }
}

/**
 * 获取缓存统计
 * @returns {Promise<{total: number, size: number}>}
 */
async function stats() {
  try {
    const allCaches = await window.__TAURI__?.persistentCache?.list?.();
    const appCaches = allCaches?.filter(item => item.key.startsWith('app:')) || [];
    
    const totalSize = appCaches.reduce((sum, item) => sum + (item.size || 0), 0);
    
    return {
      total: appCaches.length,
      size: totalSize
    };
  } catch (e) {
    console.error('[AppCache] Failed to get stats:', e);
    return { total: 0, size: 0 };
  }
}

// 导出 API
export const appCache = {
  precache,
  getFromCache,
  isCached,
  clear,
  stats,
  getAppId,
  getAppBaseUrl
};

// 为了兼容性，保留空函数
export function initAppCache() {
  console.log('[AppCache] Manual cache mode - call precache() to cache resources');
}