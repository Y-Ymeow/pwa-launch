/**
 * Persistent Cache API
 * 
 * 完全独立于 WebView 缓存的持久化存储系统
 * 特性：
 * 1. 数据存储在应用数据目录，不会被系统清理
 * 2. 支持设置过期时间（TTL）或永不过期
 * 3. 只有通过特定 API 或清除按钮才能清除
 * 4. 支持文本和二进制数据
 * 
 * 使用示例：
 * 
 * // 存储文本
 * await __TAURI__.persistentCache.setItem('config', JSON.stringify({theme: 'dark'}), {
 *   mimeType: 'application/json'
 * });
 * 
 * // 存储二进制（如图片）
 * const blob = await fetch(imageUrl).then(r => r.blob());
 * await __TAURI__.persistentCache.setItem('avatar', blob, {
 *   mimeType: 'image/png'
 * });
 * 
 * // 读取
 * const data = await __TAURI__.persistentCache.getItem('config');
 * if (data) {
 *   const config = JSON.parse(data.data); // data.data 是 base64 字符串
 * }
 * 
 * // 带过期时间的缓存（7天）
 * await __TAURI__.persistentCache.setItem('temp_data', '...', {
 *   ttl: 7 * 24 * 60 * 60 // 秒
 * });
 * 
 * // 列出所有缓存
 * const list = await __TAURI__.persistentCache.list();
 * 
 * // 删除单个
 * await __TAURI__.persistentCache.removeItem('config');
 * 
 * // 清空所有（用户主动操作）
 * await __TAURI__.persistentCache.clear();
 */

/**
 * 将 Blob 转换为 base64
 */
async function blobToBase64(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      const base64 = reader.result.split(',')[1];
      resolve(base64);
    };
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });
}

/**
 * 将 base64 转换为 Blob
 */
function base64ToBlob(base64, mimeType = '') {
  const byteCharacters = atob(base64);
  const byteArrays = [];
  
  for (let i = 0; i < byteCharacters.length; i += 512) {
    const slice = byteCharacters.slice(i, i + 512);
    const byteNumbers = new Array(slice.length);
    for (let j = 0; j < slice.length; j++) {
      byteNumbers[j] = slice.charCodeAt(j);
    }
    byteArrays.push(new Uint8Array(byteNumbers));
  }
  
  return new Blob(byteArrays, { type: mimeType });
}

/**
 * 获取当前应用 ID
 */
function getAppId() {
  try {
    const contextCookie = document.cookie
      .split(";")
      .find((c) => c.trim().startsWith("pwa_context="));
    if (contextCookie) {
      const ctx = contextCookie.trim().substring("pwa_context=".length);
      return ctx.split("/")[1] || ctx;
    }
  } catch (e) {}
  const match = window.location.href.match(/\/(https|http)\/([^/]+)/);
  return match ? match[2] : window.location.hostname || "default";
}

/**
 * 创建持久化缓存 API
 */
export function createPersistentCache(bridge) {
  return {
    /**
     * 存储数据到持久化缓存
     * 
     * @param {string} key - 缓存键
     * @param {string|Blob|ArrayBuffer|Object} value - 要存储的数据
     * @param {Object} options - 选项
     * @param {string} options.mimeType - MIME 类型（默认根据数据类型自动检测）
     * @param {number} options.ttl - 生存时间（秒），不设置则永不过期
     * @returns {Promise<boolean>} 是否成功
     * 
     * @example
     * // 存储 JSON
     * await persistentCache.setItem('user', { name: 'John' });
     * 
     * // 存储图片（永不过期）
     * await persistentCache.setItem('logo', blob, { mimeType: 'image/png' });
     * 
     * // 存储临时数据（1小时过期）
     * await persistentCache.setItem('temp', 'data', { ttl: 3600 });
     */
    async setItem(key, value, options = {}) {
      let data;
      let mimeType = options.mimeType;

      // 处理不同类型的数据
      if (value instanceof Blob) {
        data = await blobToBase64(value);
        mimeType = mimeType || value.type || 'application/octet-stream';
      } else if (value instanceof ArrayBuffer) {
        const blob = new Blob([value]);
        data = await blobToBase64(blob);
        mimeType = mimeType || 'application/octet-stream';
      } else if (typeof value === 'object') {
        data = btoa(JSON.stringify(value));
        mimeType = mimeType || 'application/json';
      } else {
        // 字符串或其他
        data = btoa(String(value));
        mimeType = mimeType || 'text/plain';
      }

      const result = await bridge.invoke("cache_set", {
        appId: getAppId(),
        key,
        data,
        options: {
          mime_type: mimeType,
          ttl: options.ttl,
        },
      });

      return result.success;
    },

    /**
     * 从持久化缓存读取数据
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<{key: string, data: string, mimeType: string, size: number, createdAt: number, updatedAt: number, expiresAt: number|null}|null>}
     *   - data: base64 编码的数据
     *   - 如果缓存不存在或已过期，返回 null
     * 
     * @example
     * const result = await persistentCache.getItem('user');
     * if (result) {
     *   const user = JSON.parse(atob(result.data));
     * }
     */
    async getItem(key) {
      const result = await bridge.invoke("cache_get", {
        appId: getAppId(),
        key,
      });

      if (result.success && result.data) {
        return {
          key: result.data.key,
          data: result.data.data,
          mimeType: result.data.mime_type,
          size: result.data.size,
          createdAt: result.data.created_at,
          updatedAt: result.data.updated_at,
          expiresAt: result.data.expires_at,
        };
      }

      return null;
    },

    /**
     * 获取缓存项作为 Blob（方便图片等二进制数据使用）
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<Blob|null>}
     * 
     * @example
     * const blob = await persistentCache.getBlob('avatar');
     * if (blob) {
     *   const url = URL.createObjectURL(blob);
     *   img.src = url;
     * }
     */
    async getBlob(key) {
      const result = await this.getItem(key);
      if (!result) return null;
      return base64ToBlob(result.data, result.mimeType);
    },

    /**
     * 获取缓存项作为文本
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<string|null>}
     */
    async getText(key) {
      const result = await this.getItem(key);
      if (!result) return null;
      return atob(result.data);
    },

    /**
     * 获取缓存项作为 JSON
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<Object|null>}
     */
    async getJSON(key) {
      const text = await this.getText(key);
      if (!text) return null;
      try {
        return JSON.parse(text);
      } catch (e) {
        console.error("[PersistentCache] Failed to parse JSON:", e);
        return null;
      }
    },

    /**
     * 删除缓存项
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<boolean>}
     */
    async removeItem(key) {
      const result = await bridge.invoke("cache_delete", {
        appId: getAppId(),
        key,
      });
      return result.success;
    },

    /**
     * 列出所有缓存项
     * 
     * @returns {Promise<Array<{key: string, mimeType: string, size: number, createdAt: number, updatedAt: number, expiresAt: number|null}>>}
     */
    async list() {
      const result = await bridge.invoke("cache_list", {
        appId: getAppId(),
      });

      if (result.success && result.data) {
        return result.data.map(item => ({
          key: item.key,
          mimeType: item.mime_type,
          size: item.size,
          createdAt: item.created_at,
          updatedAt: item.updated_at,
          expiresAt: item.expires_at,
        }));
      }

      return [];
    },

    /**
     * 清除所有缓存（⚠️ 危险操作，用户主动触发）
     * 
     * @returns {Promise<boolean>}
     */
    async clear() {
      const result = await bridge.invoke("cache_clear", {
        appId: getAppId(),
      });
      return result.success;
    },

    /**
     * 检查缓存是否存在且未过期
     * 
     * @param {string} key - 缓存键
     * @returns {Promise<boolean>}
     */
    async exists(key) {
      const result = await bridge.invoke("cache_exists", {
        appId: getAppId(),
        key,
      });
      return result.success ? result.data : false;
    },

    /**
     * 获取缓存统计信息
     * 
     * @returns {Promise<{totalEntries: number, validEntries: number, expiredEntries: number, totalSizeBytes: number, totalSizeMb: number}>}
     */
    async stats() {
      const result = await bridge.invoke("cache_stats", {
        appId: getAppId(),
      });

      if (result.success && result.data) {
        return {
          totalEntries: result.data.total_entries,
          validEntries: result.data.valid_entries,
          expiredEntries: result.data.expired_entries,
          totalSizeBytes: result.data.total_size_bytes,
          totalSizeMb: result.data.total_size_mb,
        };
      }

      return {
        totalEntries: 0,
        validEntries: 0,
        expiredEntries: 0,
        totalSizeBytes: 0,
        totalSizeMb: 0,
      };
    },

    /**
     * 缓存装饰器 - 自动缓存函数结果
     * 
     * @param {Function} fn - 要缓存的异步函数
     * @param {Object} options - 选项
     * @param {string} options.key - 缓存键（可选，默认使用函数名）
     * @param {number} options.ttl - 过期时间（秒）
     * @returns {Function}
     * 
     * @example
     * const fetchUser = persistentCache.memoize(async (userId) => {
     *   const res = await fetch(`/api/users/${userId}`);
     *   return res.json();
     * }, { ttl: 3600 });
     * 
     * // 第一次调用会执行函数并缓存
     * const user1 = await fetchUser(123);
     * // 第二次调用直接返回缓存（未过期时）
     * const user2 = await fetchUser(123);
     */
    memoize(fn, options = {}) {
      const cacheKey = options.key || fn.name || 'memoized';
      const ttl = options.ttl;

      return async function (...args) {
        const key = `${cacheKey}:${JSON.stringify(args)}`;
        
        // 尝试从缓存读取
        const cached = await this.getJSON(key);
        if (cached !== null) {
          console.log(`[PersistentCache] Cache hit: ${key}`);
          return cached;
        }

        // 执行函数
        console.log(`[PersistentCache] Cache miss: ${key}`);
        const result = await fn.apply(this, args);

        // 存入缓存
        await this.setItem(key, result, { ttl });

        return result;
      }.bind(this);
    },
  };
}
