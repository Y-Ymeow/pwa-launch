/**
 * SQLite EAV Storage - 替代 IndexedDB 的灵活实现
 *
 * 使用 EAV（Entity-Attribute-Value）模型：
 * - 不需要创建物理表
 * - 支持复杂查询（WHERE、JOIN、BETWEEN、ORDER BY）
 * - 自动按 pwa_id 隔离（后端自动获取，无需前端传递）
 */

/**
 * 创建 EAV 存储 API
 * @param {Object} bridge - Tauri 桥接对象
 * @param {string} pwaId - PWA ID（可选，由 App.tsx 自动注入）
 * @param {string} dbName - 数据库名称（默认 'default'）
 */
export function createEAVStorage(bridge, pwaId = null, dbName = "default") {
  return {
    pwaId: pwaId,
    dbName: dbName,

    /**
     * 插入或更新记录
     * @param {string} table - 逻辑表名
     * @param {string} dataId - 记录 ID
     * @param {Object} data - 数据对象
     * @returns {Promise<boolean>}
     */
    async upsert(table, dataId, data) {
      const result = await bridge.invoke("sqlite_upsert", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
        dataId: String(dataId),
        data,
      });
      return result.success ? result.data : false;
    },

    /**
     * 查询记录（支持过滤）
     * @param {string} table - 逻辑表名
     * @param {Object} options - 查询选项
     * @param {Object} options.filter - 过滤条件 {key: value}
     * @param {string} options.orderBy - 排序字段（默认 updated_at）
     * @param {boolean} options.desc - 是否降序
     * @param {number} options.limit - 限制数量
     * @param {number} options.offset - 偏移量
     * @returns {Promise<Array<Record>>}
     *
     * Record 格式:
     * { dataId, createdAt, updatedAt, data: { ... } }
     */
    async find(table, options = {}) {
      const result = await bridge.invoke("sqlite_find", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
        filter: options.filter || null,
        options: {
          order_by: options.orderBy || null,
          desc: options.desc || false,
          limit: options.limit || null,
          offset: options.offset || null,
        },
      });
      return result.success ? result.data : [];
    },

    /**
     * 查询单条记录
     * @param {string} table - 逻辑表名
     * @param {string} dataId - 记录 ID
     * @returns {Promise<Record|null>}
     */
    async findOne(table, dataId) {
      const result = await bridge.invoke("sqlite_find_one", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
        dataId: String(dataId),
      });
      return result.success ? result.data : null;
    },

    /**
     * 删除记录
     * @param {string} table - 逻辑表名
     * @param {string} dataId - 记录 ID
     * @returns {Promise<boolean>}
     */
    async delete(table, dataId) {
      const result = await bridge.invoke("sqlite_delete", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
        dataId: String(dataId),
      });
      return result.success ? result.data : false;
    },

    /**
     * 统计记录数
     * @param {string} table - 逻辑表名
     * @param {Object} filter - 过滤条件
     * @returns {Promise<number>}
     */
    async count(table, filter = null) {
      const result = await bridge.invoke("sqlite_count", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
        filter,
      });
      return result.success ? result.data : 0;
    },

    /**
     * 清空表
     * @param {string} table - 逻辑表名
     * @returns {Promise<boolean>}
     */
    async clear(table) {
      const result = await bridge.invoke("sqlite_clear_table", {
        pwaId: this.pwaId,
        dbName: this.dbName,
        tableName: table,
      });
      return result.success ? result.data : false;
    },

    /**
     * 列出所有表
     * @returns {Promise<string[]>}
     */
    async listTables() {
      const result = await bridge.invoke("sqlite_list_tables", {
        pwaId: this.pwaId,
        dbName: this.dbName,
      });
      return result.success ? result.data : [];
    },

    // ============== 便捷方法 ==============

    /**
     * 简单的键值存储（兼容 localStorage 风格）
     * @param {string} key
     * @param {any} value
     * @returns {Promise<boolean>}
     */
    async setItem(key, value) {
      return this.upsert("kv", key, { value });
    },

    /**
     * 获取键值
     * @param {string} key
     * @returns {Promise<any>}
     */
    async getItem(key) {
      const record = await this.findOne("kv", key);
      return record?.data?.value ?? null;
    },

    /**
     * 删除键值
     * @param {string} key
     * @returns {Promise<boolean>}
     */
    async removeItem(key) {
      return this.delete("kv", key);
    },

    /**
     * 获取所有键
     * @returns {Promise<string[]>}
     */
    async keys() {
      const records = await this.find("kv");
      return records.map((r) => r.dataId);
    },

    /**
     * 清空所有键值
     * @returns {Promise<boolean>}
     */
    async clearAll() {
      return this.clear("kv");
    },
  };
}

/**
 * 兼容旧版 API：创建 SQLite 存储（单表操作）
 * @deprecated 使用 createEAVStorage 替代
 */
export function createSQLiteStorage(bridge) {
  const storage = createEAVStorage(bridge);

  return {
    async setItem(key, value) {
      return storage.setItem(key, value);
    },
    async getItem(key) {
      return storage.getItem(key);
    },
    async removeItem(key) {
      return storage.removeItem(key);
    },
    async clear() {
      return storage.clearAll();
    },
    async keys() {
      return storage.keys();
    },
  };
}

/**
 * 兼容旧版 API：创建表操作
 * @deprecated 使用 createEAVStorage 替代
 */
export function createSQLiteTable(bridge) {
  const storage = createEAVStorage(bridge);

  return {
    async createTable(table) {
      return true;
    }, // EAV 不需要创建表
    async dropTable(table) {
      return storage.clear(table);
    },
    async setItem(table, key, value) {
      return storage.upsert(table, key, { value });
    },
    async getItem(table, key) {
      const record = await storage.findOne(table, key);
      return record?.data?.value ?? null;
    },
    async removeItem(table, key) {
      return storage.delete(table, key);
    },
    async keys(table) {
      const records = await storage.find(table);
      return records.map((r) => r.dataId);
    },
    async clear(table) {
      return storage.clear(table);
    },
  };
}

/**
 * 劫持 localStorage 使用 SQLite KV
 * @param {Object} bridge - Tauri 桥接对象
 */
export function hijackLocalStorage(bridge) {
  // 使用简单的 KV 命令（appId 由 App.tsx 自动注入）
  Object.defineProperty(window, "localStorage", {
    value: {
      getItem: async (key) => {
        const result = await bridge.invoke("kv_get", { key });
        return result.success ? result.data : null;
      },
      setItem: async (key, value) => {
        await bridge.invoke("kv_set", { key, value: String(value) });
      },
      removeItem: async (key) => {
        await bridge.invoke("kv_remove", { key });
      },
      clear: async () => {
        await bridge.invoke("kv_clear", {});
      },
      key: async (index) => {
        const result = await bridge.invoke("kv_get_all", {});
        if (result.success && result.data) {
          const keys = Object.keys(result.data);
          return keys[index] || null;
        }
        return null;
      },
      get length() {
        return (async () => {
          const result = await bridge.invoke("kv_get_all", {});
          return result.success && result.data
            ? Object.keys(result.data).length
            : 0;
        })();
      },
    },
    writable: false,
  });

  console.log("[Adapt] localStorage hijacked with SQLite KV");
}

/**
 * 劫持 IndexedDB 使用 SQLite
 * @param {Object} bridge - Tauri 桥接对象
 */
export function hijackIndexedDB(bridge) {
  const storage = createEAVStorage(bridge);

  // 创建一个模拟的 IndexedDB 接口
  const mockIndexedDB = {
    open: (dbName, version) => {
      const request = {
        result: {
          createObjectStore: (tableName) => ({ name: tableName }),
          transaction: (tables, mode) => ({
            objectStore: (tableName) => ({
              put: (data, key) => {
                storage.upsert(tableName, key || data.id, data);
                return { onsuccess: null, onerror: null };
              },
              get: (key) => ({
                onsuccess: null,
                onerror: null,
                result: storage.findOne(tableName, key),
              }),
              delete: (key) => {
                storage.delete(tableName, key);
                return { onsuccess: null, onerror: null };
              },
              getAll: () => ({
                onsuccess: null,
                onerror: null,
                result: storage.find(tableName),
              }),
            }),
          }),
        },
        onsuccess: null,
        onerror: null,
        onupgradeneeded: null,
      };

      // 异步触发 onsuccess
      setTimeout(() => request.onsuccess?.({ target: request }), 0);

      return request;
    },
    deleteDatabase: () => ({ onsuccess: null, onerror: null }),
  };

  Object.defineProperty(window, "indexedDB", {
    value: mockIndexedDB,
    writable: false,
  });

  console.log("[Adapt] IndexedDB hijacked with SQLite EAV");
}

// 默认导出
export default {
  createEAVStorage,
  createSQLiteStorage,
  createSQLiteTable,
  hijackLocalStorage,
  hijackIndexedDB,
};
