/**
 * SQL Database Access for PWA - 通过 postMessage 使用 Tauri SQL 插件
 * 每个 PWA 有自己的独立数据库文件
 */

// 生成唯一请求 ID
const generateRequestId = () =>
  Date.now().toString(36) + Math.random().toString(36).substr(2, 9);

/**
 * 通过 postMessage 发送 SQL 请求到父窗口
 * @param {string} pwaId - PWA ID
 * @param {string} sql - SQL 语句
 * @param {Array} params - 参数数组
 * @returns {Promise<any>}
 */
function sendSQLRequest(pwaId, sql, params = []) {
  return new Promise((resolve, reject) => {
    const requestId = generateRequestId();

    const handler = (event) => {
      if (
        event.data?.type === "ADAPT_SQL_RESPONSE" &&
        event.data?.requestId === requestId
      ) {
        window.removeEventListener("message", handler);

        if (event.data.success) {
          resolve(event.data.data);
        } else {
          reject(new Error(event.data.error));
        }
      }
    };

    window.addEventListener("message", handler);

    // 30 秒超时
    setTimeout(() => {
      window.removeEventListener("message", handler);
      reject(new Error("SQL request timeout"));
    }, 30000);

    window.parent.postMessage(
      {
        type: "ADAPT_SQL_REQUEST",
        requestId,
        pwaId,
        sql,
        params,
      },
      "*",
    );
  });
}

/**
 * 创建 SQL 数据库连接
 * @param {string} pwaId - PWA ID
 * @returns {Object} SQL 数据库接口
 */
export function createSQL(pwaId) {
  return {
    pwaId,

    /**
     * 执行 SQL 查询（返回结果集）
     * @param {string} sql - SQL 语句
     * @param {Array} params - 参数
     * @returns {Promise<Array>}
     */
    async select(sql, params = []) {
      return sendSQLRequest(this.pwaId, sql, params);
    },

    /**
     * 执行 SQL 语句（无返回值）
     * @param {string} sql - SQL 语句
     * @param {Array} params - 参数
     * @returns {Promise<void>}
     */
    async execute(sql, params = []) {
      return sendSQLRequest(this.pwaId, sql, params);
    },

    /**
     * 批量执行
     * @param {Array<{sql: string, params: Array}>} statements
     * @returns {Promise<void>}
     */
    async batch(statements) {
      for (const stmt of statements) {
        await sendSQLRequest(this.pwaId, stmt.sql, stmt.params || []);
      }
    },
  };
}

/**
 * 创建 EAV 存储（替代 IndexedDB）
 * @param {string} pwaId - PWA ID
 * @param {string} dbName - 数据库名称（默认 'default'）
 */
export function createEAV(pwaId, dbName = "default") {
  const sql = createSQL(pwaId);

  // 初始化表结构
  async function initTables() {
    await sql.execute(`
      CREATE TABLE IF NOT EXISTS pwa_data (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        db_name TEXT NOT NULL,
        table_name TEXT NOT NULL,
        data_id TEXT NOT NULL,
        created_at INTEGER DEFAULT (strftime('%s', 'now')),
        updated_at INTEGER DEFAULT (strftime('%s', 'now')),
        UNIQUE(db_name, table_name, data_id)
      )
    `);

    await sql.execute(`
      CREATE TABLE IF NOT EXISTS pwa_schema_data (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        data_id INTEGER NOT NULL,
        attr_name TEXT NOT NULL,
        attr_value TEXT,
        FOREIGN KEY (data_id) REFERENCES pwa_data(id) ON DELETE CASCADE
      )
    `);

    await sql.execute(`
      CREATE INDEX IF NOT EXISTS idx_pwa_data_lookup 
      ON pwa_data(db_name, table_name, data_id)
    `);

    await sql.execute(`
      CREATE INDEX IF NOT EXISTS idx_pwa_schema_data_data_id 
      ON pwa_schema_data(data_id)
    `);
  }

  // 延迟初始化
  let initialized = false;
  async function ensureInit() {
    if (!initialized) {
      await initTables();
      initialized = true;
    }
  }

  return {
    pwaId,
    dbName,

    /**
     * 插入或更新记录
     */
    async upsert(table, dataId, data) {
      await ensureInit();

      const result = await sql.select(
        `SELECT id FROM pwa_data 
         WHERE db_name = ? AND table_name = ? AND data_id = ?`,
        [this.dbName, table, String(dataId)],
      );

      let rowId;
      if (result.length === 0) {
        await sql.execute(
          `INSERT INTO pwa_data (db_name, table_name, data_id, updated_at) 
           VALUES (?, ?, ?, strftime('%s', 'now'))`,
          [this.dbName, table, String(dataId)],
        );
        const newRow = await sql.select(
          `SELECT id FROM pwa_data 
           WHERE db_name = ? AND table_name = ? AND data_id = ?`,
          [this.dbName, table, String(dataId)],
        );
        rowId = newRow[0].id;
      } else {
        rowId = result[0].id;
        await sql.execute(
          `UPDATE pwa_data SET updated_at = strftime('%s', 'now') WHERE id = ?`,
          [rowId],
        );
        await sql.execute(`DELETE FROM pwa_schema_data WHERE data_id = ?`, [
          rowId,
        ]);
      }

      for (const [key, value] of Object.entries(data)) {
        await sql.execute(
          `INSERT INTO pwa_schema_data (data_id, attr_name, attr_value) 
           VALUES (?, ?, ?)`,
          [rowId, key, JSON.stringify(value)],
        );
      }

      return true;
    },

    /**
     * 查询记录
     * @param {string} table - 表名
     * @param {Object} options - 查询选项
     * @param {Object} options.where - 过滤条件
     *   - 简单: { status: 'active', count: 5 }
     *   - 操作符: { age: { $gt: 18, $lt: 60 } }
     *   - 数组($in): { status: ['active', 'pending'] } 或 { status: { $in: ['active', 'pending'] } }
     *   - 包含($contains): { name: { $contains: 'John' } }
     *   - 开头($startswith): { email: { $startswith: 'admin' } }
     *   - 结尾($endswith): { email: { $endswith: '@gmail.com' } }
     *   - 不等于($ne): { status: { $ne: 'deleted' } }
     *   - 支持: $eq, $ne, $gt, $gte, $lt, $lte, $in, $nin, $contains, $startswith, $endswith
     * @param {string|Array<{field: string, order?: 'asc'|'desc'}>} options.sort - 排序配置
     *   - 简写: 'createdAt' 或 { field: 'createdAt', order: 'desc' }
     *   - 多字段: [{ field: 'priority', order: 'desc' }, { field: 'createdAt', order: 'asc' }]
     * @param {string} options.orderBy - 兼容旧版：排序字段
     * @param {boolean} options.desc - 兼容旧版：是否降序
     * @param {number} options.limit - 限制数量
     * @param {number} options.offset - 偏移量
     * @returns {Promise<Array>} 记录列表
     */
    async find(table, options = {}) {
      await ensureInit();

      let sqlQuery = `
        SELECT d.data_id, d.created_at, d.updated_at, s.attr_name, s.attr_value
        FROM pwa_data d
        LEFT JOIN pwa_schema_data s ON d.id = s.data_id
        WHERE d.db_name = ? AND d.table_name = ?
      `;
      const params = [this.dbName, table];

      // 处理 where 条件
      const whereConditions = [];

      function buildWhereCondition(field, condition, isSystemField) {
        const tablePrefix = isSystemField ? "d" : "s2";
        const columnName = isSystemField ? field : "attr_value";

        if (condition === null || condition === undefined) {
          // NULL 检查
          return { sql: `${tablePrefix}.${columnName} IS NULL`, params: [] };
        }

        if (typeof condition === "object" && !Array.isArray(condition)) {
          // 操作符对象: { $gt: 10, $lt: 100 }
          const ops = [];
          const opParams = [];

          for (const [op, val] of Object.entries(condition)) {
            switch (op) {
              case "$eq":
                ops.push(`${tablePrefix}.${columnName} = ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$ne":
                ops.push(`${tablePrefix}.${columnName} != ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$gt":
                ops.push(`${tablePrefix}.${columnName} > ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$gte":
                ops.push(`${tablePrefix}.${columnName} >= ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$lt":
                ops.push(`${tablePrefix}.${columnName} < ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$lte":
                ops.push(`${tablePrefix}.${columnName} <= ?`);
                opParams.push(isSystemField ? val : JSON.stringify(val));
                break;
              case "$in":
                if (Array.isArray(val) && val.length > 0) {
                  const placeholders = val.map(() => "?").join(",");
                  ops.push(`${tablePrefix}.${columnName} IN (${placeholders})`);
                  opParams.push(
                    ...val.map((v) => (isSystemField ? v : JSON.stringify(v))),
                  );
                }
                break;
              case "$nin":
                if (Array.isArray(val) && val.length > 0) {
                  const placeholders = val.map(() => "?").join(",");
                  ops.push(
                    `${tablePrefix}.${columnName} NOT IN (${placeholders})`,
                  );
                  opParams.push(
                    ...val.map((v) => (isSystemField ? v : JSON.stringify(v))),
                  );
                }
                break;
              case "$contains":
                ops.push(`${tablePrefix}.${columnName} LIKE ?`);
                opParams.push(`%${val}%`);
                break;
              case "$startswith":
                ops.push(`${tablePrefix}.${columnName} LIKE ?`);
                opParams.push(`${val}%`);
                break;
              case "$endswith":
                ops.push(`${tablePrefix}.${columnName} LIKE ?`);
                opParams.push(`%${val}`);
                break;
            }
          }

          return { sql: ops.join(" AND "), params: opParams };
        } else if (Array.isArray(condition)) {
          // 数组简写：等价于 $in
          const placeholders = condition.map(() => "?").join(",");
          return {
            sql: `${tablePrefix}.${columnName} IN (${placeholders})`,
            params: condition.map((v) =>
              isSystemField ? v : JSON.stringify(v),
            ),
          };
        } else {
          // 简单值：等价于 $eq
          return {
            sql: `${tablePrefix}.${columnName} = ?`,
            params: [isSystemField ? condition : JSON.stringify(condition)],
          };
        }
      }

      if (options.where && typeof options.where === "object") {
        for (const [field, value] of Object.entries(options.where)) {
          const isSystemField = [
            "created_at",
            "updated_at",
            "data_id",
            "id",
          ].includes(field);

          if (isSystemField) {
            // 系统字段直接过滤
            const condition = buildWhereCondition(field, value, true);
            if (condition.sql) {
              whereConditions.push(condition.sql);
              params.push(...condition.params);
            }
          } else {
            // EAV 字段：通过子查询过滤
            const condition = buildWhereCondition(field, value, false);
            if (condition.sql) {
              whereConditions.push(`EXISTS (
                SELECT 1 FROM pwa_schema_data s2 
                WHERE s2.data_id = d.id 
                AND s2.attr_name = ? 
                AND ${condition.sql}
              )`);
              params.push(field, ...condition.params);
            }
          }
        }
      }

      // 添加 WHERE 条件到查询
      if (whereConditions.length > 0) {
        sqlQuery += ` AND ${whereConditions.join(" AND ")}`;
      }

      // 字段名映射：驼峰 -> 下划线
      const fieldMapping = {
        createdAt: "created_at",
        updatedAt: "updated_at",
        dataId: "data_id",
        id: "id",
      };

      // 转换字段名
      function normalizeField(field) {
        return fieldMapping[field] || field;
      }

      // 处理排序
      const sortClauses = [];

      // 新版 sort 参数
      if (options.sort) {
        if (Array.isArray(options.sort)) {
          // 多字段排序: [{ field: 'priority', order: 'desc' }, { field: 'name', order: 'asc' }]
          for (const sortItem of options.sort) {
            const rawField =
              typeof sortItem === "string" ? sortItem : sortItem.field;
            const field = normalizeField(rawField);
            const order =
              typeof sortItem === "string" ? "asc" : sortItem.order || "asc";

            if (["created_at", "updated_at", "data_id", "id"].includes(field)) {
              sortClauses.push(`d.${field} ${order.toUpperCase()}`);
            } else {
              // 对嵌套数据排序需要特殊处理，这里先按 updated_at 排
              // 复杂排序可以在内存中进行
            }
          }
        } else if (typeof options.sort === "string") {
          // 单字段简写: 'createdAt'
          const field = normalizeField(options.sort);
          if (["created_at", "updated_at", "data_id", "id"].includes(field)) {
            sortClauses.push(`d.${field} ASC`);
          }
        } else if (typeof options.sort === "object") {
          // 单字段对象: { field: 'createdAt', order: 'desc' }
          const field = normalizeField(options.sort.field);
          const order = options.sort.order || "asc";
          if (["created_at", "updated_at", "data_id", "id"].includes(field)) {
            sortClauses.push(`d.${field} ${order.toUpperCase()}`);
          }
        }
      }

      // 兼容旧版 orderBy/desc
      if (sortClauses.length === 0 && options.orderBy) {
        const order = options.desc ? "DESC" : "ASC";
        sortClauses.push(`d.${options.orderBy} ${order}`);
      }

      // 默认排序
      if (sortClauses.length === 0) {
        sortClauses.push("d.updated_at DESC");
      }

      sqlQuery += ` ORDER BY ${sortClauses.join(", ")}`;

      if (options.limit) {
        sqlQuery += ` LIMIT ${options.limit}`;
      }
      if (options.offset) {
        sqlQuery += ` OFFSET ${options.offset}`;
      }

      const rows = await sql.select(sqlQuery, params);

      const records = new Map();
      for (const row of rows) {
        if (!records.has(row.data_id)) {
          records.set(row.data_id, {
            dataId: row.data_id,
            createdAt: row.created_at,
            updatedAt: row.updated_at,
            data: {},
          });
        }
        if (row.attr_name) {
          try {
            records.get(row.data_id).data[row.attr_name] = JSON.parse(
              row.attr_value,
            );
          } catch {
            records.get(row.data_id).data[row.attr_name] = row.attr_value;
          }
        }
      }

      let results = Array.from(records.values());

      // 对嵌套数据进行内存排序（如果 sort 包含非系统字段）
      if (options.sort && Array.isArray(options.sort)) {
        const hasNestedSort = options.sort.some((item) => {
          const field = typeof item === "string" ? item : item.field;
          return !["created_at", "updated_at", "data_id", "id"].includes(field);
        });

        if (hasNestedSort) {
          results = this._sortInMemory(results, options.sort);
        }
      }

      return results;
    },

    /**
     * 内存排序（用于嵌套数据字段）
     * @private
     */
    _sortInMemory(records, sortConfig) {
      return records.sort((a, b) => {
        for (const sortItem of sortConfig) {
          const field =
            typeof sortItem === "string" ? sortItem : sortItem.field;
          const order =
            typeof sortItem === "string" ? "asc" : sortItem.order || "asc";

          let valA = a.data[field] ?? a[field];
          let valB = b.data[field] ?? b[field];

          // 类型比较
          let comparison = 0;
          if (typeof valA === "number" && typeof valB === "number") {
            comparison = valA - valB;
          } else if (valA instanceof Date && valB instanceof Date) {
            comparison = valA.getTime() - valB.getTime();
          } else {
            const strA = String(valA || "").toLowerCase();
            const strB = String(valB || "").toLowerCase();
            comparison = strA < strB ? -1 : strA > strB ? 1 : 0;
          }

          if (order.toLowerCase() === "desc") {
            comparison = -comparison;
          }

          if (comparison !== 0) {
            return comparison;
          }
        }
        return 0;
      });
    },

    /**
     * 查询单条记录
     * @param {string} table - 表名
     * @param {string|Object} dataIdOrOptions - 记录ID 或 查询选项
     *   - 简单: 'record-id'
     *   - 对象: { dataId: 'record-id' } 或 { where: { name: 'John' } }
     * @returns {Promise<Object|null>} 记录对象或 null
     */
    async findOne(table, dataIdOrOptions) {
      await ensureInit();

      let dataId = null;
      let where = null;

      if (typeof dataIdOrOptions === "string") {
        dataId = dataIdOrOptions;
      } else if (dataIdOrOptions && typeof dataIdOrOptions === "object") {
        dataId = dataIdOrOptions.dataId;
        where = dataIdOrOptions.where;
      }

      let sqlQuery = `
        SELECT d.data_id, d.created_at, d.updated_at, s.attr_name, s.attr_value
        FROM pwa_data d
        LEFT JOIN pwa_schema_data s ON d.id = s.data_id
        WHERE d.db_name = ? AND d.table_name = ?
      `;
      const params = [this.dbName, table];

      // 字段名映射（驼峰 -> 下划线）
      const fieldMapping = {
        createdAt: "created_at",
        updatedAt: "updated_at",
        dataId: "data_id",
        id: "id",
      };

      function normalizeField(field) {
        return fieldMapping[field] || field;
      }

      if (dataId) {
        sqlQuery += ` AND d.data_id = ?`;
        params.push(String(dataId));
      } else if (where) {
        // 复用 find 中的 where 构建逻辑
        const whereConditions = [];

        function buildWhereCondition(field, condition, isSystemField) {
          const tablePrefix = isSystemField ? "d" : "s2";
          const columnName = isSystemField ? field : "attr_value";

          if (condition === null || condition === undefined) {
            return { sql: `${tablePrefix}.${columnName} IS NULL`, params: [] };
          }

          if (typeof condition === "object" && !Array.isArray(condition)) {
            const ops = [];
            const opParams = [];

            for (const [op, val] of Object.entries(condition)) {
              switch (op) {
                case "$eq":
                  ops.push(`${tablePrefix}.${columnName} = ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$ne":
                  ops.push(`${tablePrefix}.${columnName} != ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$gt":
                  ops.push(`${tablePrefix}.${columnName} > ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$gte":
                  ops.push(`${tablePrefix}.${columnName} >= ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$lt":
                  ops.push(`${tablePrefix}.${columnName} < ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$lte":
                  ops.push(`${tablePrefix}.${columnName} <= ?`);
                  opParams.push(isSystemField ? val : JSON.stringify(val));
                  break;
                case "$in":
                  if (Array.isArray(val) && val.length > 0) {
                    const placeholders = val.map(() => "?").join(",");
                    ops.push(
                      `${tablePrefix}.${columnName} IN (${placeholders})`,
                    );
                    opParams.push(
                      ...val.map((v) =>
                        isSystemField ? v : JSON.stringify(v),
                      ),
                    );
                  }
                  break;
                case "$contains":
                  ops.push(`${tablePrefix}.${columnName} LIKE ?`);
                  opParams.push(`%${val}%`);
                  break;
              }
            }

            return { sql: ops.join(" AND "), params: opParams };
          } else if (Array.isArray(condition)) {
            const placeholders = condition.map(() => "?").join(",");
            return {
              sql: `${tablePrefix}.${columnName} IN (${placeholders})`,
              params: condition.map((v) =>
                isSystemField ? v : JSON.stringify(v),
              ),
            };
          } else {
            return {
              sql: `${tablePrefix}.${columnName} = ?`,
              params: [isSystemField ? condition : JSON.stringify(condition)],
            };
          }
        }

        for (const [rawField, value] of Object.entries(where)) {
          const field = normalizeField(rawField);
          const isSystemField = [
            "created_at",
            "updated_at",
            "data_id",
            "id",
          ].includes(field);

          if (isSystemField) {
            const condition = buildWhereCondition(field, value, true);
            if (condition.sql) {
              whereConditions.push(condition.sql);
              params.push(...condition.params);
            }
          } else {
            const condition = buildWhereCondition(field, value, false);
            if (condition.sql) {
              whereConditions.push(`EXISTS (
                SELECT 1 FROM pwa_schema_data s2 
                WHERE s2.data_id = d.id 
                AND s2.attr_name = ? 
                AND ${condition.sql}
              )`);
              params.push(field, ...condition.params);
            }
          }
        }

        if (whereConditions.length > 0) {
          sqlQuery += ` AND ${whereConditions.join(" AND ")}`;
        }
      }

      sqlQuery += ` LIMIT 1`;

      const rows = await sql.select(sqlQuery, params);

      if (rows.length === 0) return null;

      const record = {
        dataId: rows[0].data_id,
        createdAt: rows[0].created_at,
        updatedAt: rows[0].updated_at,
        data: {},
      };

      for (const row of rows) {
        if (row.attr_name) {
          try {
            record.data[row.attr_name] = JSON.parse(row.attr_value);
          } catch {
            record.data[row.attr_name] = row.attr_value;
          }
        }
      }

      return record;
    },

    /**
     * 删除记录
     */
    async delete(table, dataId) {
      await ensureInit();

      await sql.execute(
        `DELETE FROM pwa_data 
         WHERE db_name = ? AND table_name = ? AND data_id = ?`,
        [this.dbName, table, String(dataId)],
      );

      return true;
    },

    /**
     * 清空表
     */
    async clear(table) {
      await ensureInit();

      await sql.execute(
        `DELETE FROM pwa_data 
         WHERE db_name = ? AND table_name = ?`,
        [this.dbName, table],
      );

      return true;
    },

    /**
     * 列出所有表
     * @returns {Promise<string[]>} 表名列表
     */
    async listTables() {
      await ensureInit();

      const rows = await sql.select(
        `SELECT DISTINCT table_name FROM pwa_data WHERE db_name = ?`,
        [this.dbName],
      );

      return rows.map((row) => row.table_name);
    },

    /**
     * 统计表中记录数
     * @param {string} table - 表名
     * @returns {Promise<number>} 记录数
     */
    async count(table) {
      await ensureInit();

      const result = await sql.select(
        `SELECT COUNT(*) as count FROM pwa_data 
         WHERE db_name = ? AND table_name = ?`,
        [this.dbName, table],
      );

      return result[0]?.count || 0;
    },

    /**
     * 删除整个表（包括表结构和数据）
     * @param {string} table - 表名
     * @returns {Promise<boolean>}
     */
    async deleteTable(table) {
      await ensureInit();

      // 先删除关联的 schema_data
      await sql.execute(
        `DELETE FROM pwa_schema_data 
         WHERE data_id IN (
           SELECT id FROM pwa_data 
           WHERE db_name = ? AND table_name = ?
         )`,
        [this.dbName, table],
      );

      // 再删除 pwa_data 中的记录
      await sql.execute(
        `DELETE FROM pwa_data 
         WHERE db_name = ? AND table_name = ?`,
        [this.dbName, table],
      );

      return true;
    },

    // ============== KV 便捷方法 ==============

    async setItem(key, value) {
      await ensureInit();

      await sql.execute(
        `CREATE TABLE IF NOT EXISTS kv_store (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          updated_at INTEGER DEFAULT (strftime('%s', 'now'))
        )`,
      );

      await sql.execute(
        `INSERT OR REPLACE INTO kv_store (key, value, updated_at) 
         VALUES (?, ?, strftime('%s', 'now'))`,
        [key, JSON.stringify(value)],
      );

      return true;
    },

    async getItem(key) {
      await ensureInit();

      const rows = await sql.select(
        `SELECT value FROM kv_store WHERE key = ?`,
        [key],
      );

      if (rows.length === 0) return null;

      try {
        return JSON.parse(rows[0].value);
      } catch {
        return rows[0].value;
      }
    },

    async removeItem(key) {
      await ensureInit();
      await sql.execute(`DELETE FROM kv_store WHERE key = ?`, [key]);
      return true;
    },
  };
}

/**
 * 从 URL 参数获取 pwaId
 * @returns {string} pwaId
 */
function getPwaIdFromUrl() {
  try {
    const urlParams = new URLSearchParams(window.location.search);
    return urlParams.get("__pwa_id") || "default";
  } catch (e) {
    return "default";
  }
}

/**
 * 通过 postMessage 发送 Store 请求到父窗口
 * @param {string} action - 操作: get/set/delete/clear/entries
 * @param {string} key - 键名
 * @param {any} value - 值
 * @returns {Promise<any>}
 */
function sendStoreRequest(action, key = null, value = null) {
  return new Promise((resolve, reject) => {
    const requestId = generateRequestId();
    const timeout = setTimeout(() => {
      reject(new Error("Store request timeout"));
    }, 10000);

    const handler = (event) => {
      if (
        event.data?.type === "ADAPT_STORE_RESPONSE" &&
        event.data?.requestId === requestId
      ) {
        clearTimeout(timeout);
        window.removeEventListener("message", handler);

        if (event.data.success) {
          resolve(event.data.data);
        } else {
          reject(new Error(event.data.error));
        }
      }
    };

    window.addEventListener("message", handler);
    window.parent.postMessage(
      {
        type: "ADAPT_STORE_REQUEST",
        requestId,
        action,
        key,
        value,
      },
      "*",
    );
  });
}

/**
 * 劫持 localStorage 使用 Store 存储（通过 postMessage）
 */
export function hijackLocalStorage() {
  const memoryCache = new Map();
  let initialized = false;
  let initPromise = null;

  async function doInit() {
    if (initialized) return;
    try {
      // 通过 postMessage 获取所有 entries
      const entries = await sendStoreRequest("entries");
      for (const [key, value] of entries) {
        memoryCache.set(key, String(value));
      }
      initialized = true;
      console.log(`[Adapt] localStorage loaded ${entries.length} items`);
    } catch (e) {
      console.error("[Adapt] localStorage init failed:", e);
    }
  }

  function init() {
    if (!initPromise) {
      initPromise = doInit();
    }
    return initPromise;
  }

  // 启动初始化
  init();

  Object.defineProperty(window, "localStorage", {
    value: {
      getItem(key) {
        return memoryCache.get(key) ?? null;
      },
      setItem(key, value) {
        const strValue = String(value);
        memoryCache.set(key, strValue);
        // 后台写入 store
        if (initialized) {
          sendStoreRequest("set", key, strValue).catch(console.error);
        } else {
          init()
            .then(() => sendStoreRequest("set", key, strValue))
            .catch(console.error);
        }
      },
      removeItem(key) {
        memoryCache.delete(key);
        if (initialized) {
          sendStoreRequest("delete", key).catch(console.error);
        } else {
          init()
            .then(() => sendStoreRequest("delete", key))
            .catch(console.error);
        }
      },
      clear() {
        memoryCache.clear();
        if (initialized) {
          sendStoreRequest("clear").catch(console.error);
        }
      },
      key(index) {
        const keys = Array.from(memoryCache.keys());
        return keys[index] || null;
      },
      get length() {
        return memoryCache.size;
      },
    },
    writable: false,
  });

  console.log(`[Adapt] localStorage hijacked with Store (postMessage)`);
}

/**
 * 通过 postMessage 发送 Cache 请求到父窗口
 * @param {string} action - 操作: get/set/delete/clear
 * @param {string} namespace - 命名空间
 * @param {string} key - 键名
 * @param {any} value - 值
 * @returns {Promise<any>}
 */
function sendCacheRequest(
  action,
  namespace = "default",
  key = null,
  value = null,
) {
  return new Promise((resolve, reject) => {
    const requestId = generateRequestId();
    const timeout = setTimeout(() => {
      reject(new Error("Cache request timeout"));
    }, 10000);

    const handler = (event) => {
      if (
        event.data?.type === "ADAPT_CACHE_RESPONSE" &&
        event.data?.requestId === requestId
      ) {
        clearTimeout(timeout);
        window.removeEventListener("message", handler);

        if (event.data.success) {
          resolve(event.data.data);
        } else {
          reject(new Error(event.data.error));
        }
      }
    };

    window.addEventListener("message", handler);
    window.parent.postMessage(
      {
        type: "ADAPT_CACHE_REQUEST",
        requestId,
        action,
        namespace,
        key,
        value,
      },
      "*",
    );
  });
}

/**
 * 创建 Cache API - 用于存储任意类型的大文件/二进制数据（通过 postMessage）
 * @returns {Object} Cache 接口
 */
export function createCache() {
  return {
    /**
     * 存储数据
     * @param {string} key - 缓存键
     * @param {any} data - 任意数据（对象、ArrayBuffer、Blob 等）
     * @returns {Promise<void>}
     */
    async set(key, data) {
      // 对于二进制数据，转为 base64 存储
      let value;
      if (data instanceof ArrayBuffer) {
        const bytes = new Uint8Array(data);
        let binary = "";
        for (let i = 0; i < bytes.byteLength; i++) {
          binary += String.fromCharCode(bytes[i]);
        }
        value = {
          __type: "ArrayBuffer",
          __data: btoa(binary),
        };
      } else if (data instanceof Blob) {
        const arrayBuffer = await data.arrayBuffer();
        const bytes = new Uint8Array(arrayBuffer);
        let binary = "";
        for (let i = 0; i < bytes.byteLength; i++) {
          binary += String.fromCharCode(bytes[i]);
        }
        value = {
          __type: "Blob",
          __mimeType: data.type,
          __data: btoa(binary),
        };
      } else {
        value = {
          __type: "json",
          __data: data,
        };
      }
      await sendCacheRequest("set", "default", key, value);
    },

    /**
     * 读取数据
     * @param {string} key - 缓存键
     * @returns {Promise<any>} 原始数据
     */
    async get(key) {
      const item = await sendCacheRequest("get", "default", key);
      if (!item) return null;

      if (item.__type === "ArrayBuffer") {
        const binary = atob(item.__data);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
          bytes[i] = binary.charCodeAt(i);
        }
        return bytes.buffer;
      } else if (item.__type === "Blob") {
        const binary = atob(item.__data);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
          bytes[i] = binary.charCodeAt(i);
        }
        return new Blob([bytes.buffer], { type: item.__mimeType });
      } else {
        return item.__data;
      }
    },

    /**
     * 删除指定 key
     * @param {string} key - 缓存键
     * @returns {Promise<void>}
     */
    async delete(key) {
      await sendCacheRequest("delete", "default", key);
    },

    /**
     * 清空所有缓存
     * @returns {Promise<void>}
     */
    async clear() {
      await sendCacheRequest("clear", "default");
    },

    /**
     * 创建命名空间缓存（独立文件）
     * @param {string} namespace - 命名空间
     * @returns {Object} 命名空间缓存接口
     */
    namespace(namespace) {
      return {
        set: async (key, data) => {
          let value;
          if (data instanceof ArrayBuffer) {
            const bytes = new Uint8Array(data);
            let binary = "";
            for (let i = 0; i < bytes.byteLength; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            value = { __type: "ArrayBuffer", __data: btoa(binary) };
          } else if (data instanceof Blob) {
            const arrayBuffer = await data.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);
            let binary = "";
            for (let i = 0; i < bytes.byteLength; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            value = {
              __type: "Blob",
              __mimeType: data.type,
              __data: btoa(binary),
            };
          } else {
            value = { __type: "json", __data: data };
          }
          await sendCacheRequest("set", namespace, key, value);
        },
        get: async (key) => {
          const item = await sendCacheRequest("get", namespace, key);
          if (!item) return null;
          if (item.__type === "ArrayBuffer") {
            const binary = atob(item.__data);
            const bytes = new Uint8Array(binary.length);
            for (let i = 0; i < binary.length; i++)
              bytes[i] = binary.charCodeAt(i);
            return bytes.buffer;
          } else if (item.__type === "Blob") {
            const binary = atob(item.__data);
            const bytes = new Uint8Array(binary.length);
            for (let i = 0; i < binary.length; i++)
              bytes[i] = binary.charCodeAt(i);
            return new Blob([bytes.buffer], { type: item.__mimeType });
          } else {
            return item.__data;
          }
        },
        delete: async (key) => {
          await sendCacheRequest("delete", namespace, key);
        },
        clear: async () => {
          await sendCacheRequest("clear", namespace);
        },
      };
    },
  };
}

// 默认导出
export default {
  createSQL,
  createEAV,
  hijackLocalStorage,
  createCache,
};

