//! SQLite EAV Storage - 替代 IndexedDB 的灵活实现
//! 
//! 使用 EAV（Entity-Attribute-Value）模型：
//! - pwa_data: 存储实体（记录）基本信息
//! - pwa_schema_data: 存储具体字段值
//! - 不需要创建物理表，通过逻辑表名隔离
//! - 支持完整 SQL 查询（WHERE、JOIN、BETWEEN 等）

use std::collections::HashMap;
use rusqlite::OptionalExtension;
use crate::models::CommandResponse;

/// 初始化 EAV 表结构
pub fn init_sqlite_kv() -> Result<(), String> {
    let conn = crate::DB_CONN.get()
        .ok_or("DB_CONN 未初始化")?
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;
    
    // 实体表：存储每条记录的基本信息
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pwa_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pwa_id TEXT NOT NULL,
            db_name TEXT NOT NULL DEFAULT 'default',
            table_name TEXT NOT NULL,
            data_id TEXT NOT NULL,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            updated_at INTEGER DEFAULT (strftime('%s', 'now')),
            UNIQUE(pwa_id, db_name, table_name, data_id)
        )",
        [],
    ).map_err(|e| format!("创建 pwa_data 表失败: {}", e))?;
    
    // 属性值表：存储具体字段值（EAV 模型）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pwa_schema_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            data_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT,
            value_type TEXT DEFAULT 'string',
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (data_id) REFERENCES pwa_data(id) ON DELETE CASCADE,
            UNIQUE(data_id, key)
        )",
        [],
    ).map_err(|e| format!("创建 pwa_schema_data 表失败: {}", e))?;
    
    // 创建索引优化查询
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pwa_data_lookup ON pwa_data(pwa_id, db_name, table_name)",
        [],
    ).map_err(|e| format!("创建索引失败: {}", e))?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pwa_data_data_id ON pwa_data(data_id)",
        [],
    ).map_err(|e| format!("创建索引失败: {}", e))?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_schema_data_lookup ON pwa_schema_data(data_id, key)",
        [],
    ).map_err(|e| format!("创建索引失败: {}", e))?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_schema_data_value ON pwa_schema_data(value)",
        [],
    ).map_err(|e| format!("创建索引失败: {}", e))?;
    
    log::info!("[SQLiteEAV] 表初始化完成");
    Ok(())
}

/// 获取数据库连接
fn get_db_conn() -> Result<std::sync::MutexGuard<'static, rusqlite::Connection>, String> {
    crate::DB_CONN.get()
        .ok_or("DB_CONN 未初始化")?
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))
}

/// 验证标识符
fn sanitize_id(id: &str) -> Result<String, String> {
    if id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        Ok(id.to_string())
    } else {
        Err("无效的标识符".to_string())
    }
}

// ==================== EAV 模型核心操作 ====================

/// 插入或更新记录
#[tauri::command]
pub fn sqlite_upsert(
    pwa_id: String,
    db_name: String,
    table_name: String,
    data_id: String,
    data: HashMap<String, serde_json::Value>,
) -> Result<CommandResponse<bool>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    
    let mut conn = get_db_conn()?;
    let tx = conn.transaction()
        .map_err(|e| format!("事务开始失败: {}", e))?;
    
    // 1. 插入或更新主表
    tx.execute(
        "INSERT INTO pwa_data (pwa_id, db_name, table_name, data_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))
         ON CONFLICT(pwa_id, db_name, table_name, data_id) 
         DO UPDATE SET updated_at = strftime('%s', 'now')",
        [&pwa_id, &db_name, &table_name, &data_id],
    ).map_err(|e| format!("插入主表失败: {}", e))?;
    
    // 2. 获取内部 ID
    let internal_id: i64 = tx.query_row(
        "SELECT id FROM pwa_data 
         WHERE pwa_id = ?1 AND db_name = ?2 AND table_name = ?3 AND data_id = ?4",
        [&pwa_id, &db_name, &table_name, &data_id],
        |row| row.get(0),
    ).map_err(|e| format!("获取内部 ID 失败: {}", e))?;
    
    // 3. 插入属性值
    for (key, value) in data {
        let (value_str, value_type) = match value {
            serde_json::Value::Null => ("".to_string(), "null"),
            serde_json::Value::Bool(b) => (b.to_string(), "boolean"),
            serde_json::Value::Number(n) => (n.to_string(), "number"),
            serde_json::Value::String(s) => (s, "string"),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => 
                (value.to_string(), "json"),
        };
        
        let type_str = value_type.to_string();
        tx.execute(
            "INSERT INTO pwa_schema_data (data_id, key, value, value_type)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(data_id, key) 
             DO UPDATE SET value = ?3, value_type = ?4",
            rusqlite::params![internal_id, &key, &value_str, &type_str],
        ).map_err(|e| format!("插入属性失败: {}", e))?;
    }
    
    tx.commit().map_err(|e| format!("提交失败: {}", e))?;
    Ok(CommandResponse::success(true))
}

/// 查询记录（支持复杂条件）
#[tauri::command]
pub fn sqlite_find(
    pwa_id: String,
    db_name: String,
    table_name: String,
    filter: Option<HashMap<String, serde_json::Value>>,
    options: Option<QueryOptions>,
) -> Result<CommandResponse<Vec<Record>>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    let opts = options.unwrap_or_default();
    
    let conn = get_db_conn()?;
    
    // 构建查询 SQL
    let mut sql = format!(
        "SELECT d.id, d.data_id, d.created_at, d.updated_at 
         FROM pwa_data d 
         WHERE d.pwa_id = '{}' AND d.db_name = '{}' AND d.table_name = '{}'",
        pwa_id, db_name, table_name
    );
    
    // 添加过滤条件
    let mut params: Vec<String> = vec![];
    if let Some(f) = filter {
        for (key, value) in f {
            let value_str = json_to_string(&value);
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM pwa_schema_data s 
                   WHERE s.data_id = d.id AND s.key = '{}' AND s.value = '{}')",
                key, value_str
            ));
        }
    }
    
    // 添加排序
    if let Some(order_by) = opts.order_by {
        sql.push_str(&format!(" ORDER BY d.{}", order_by));
        if opts.desc.unwrap_or(false) {
            sql.push_str(" DESC");
        }
    }
    
    // 添加分页
    if let Some(limit) = opts.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
        if let Some(offset) = opts.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }
    }
    
    // 执行查询
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备查询失败: {}", e))?;
    let rows: Vec<(i64, String, i64, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| format!("查询失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取失败: {}", e))?;
    
    // 获取每条记录的属性
    let mut records = Vec::new();
    for (internal_id, data_id, created_at, updated_at) in rows {
        let mut record = Record {
            data_id,
            created_at,
            updated_at,
            data: HashMap::new(),
        };
        
        let mut stmt = conn.prepare(
            "SELECT key, value, value_type FROM pwa_schema_data WHERE data_id = ?1"
        ).map_err(|e| format!("准备属性查询失败: {}", e))?;
        
        let attrs: Vec<(String, String, String)> = stmt
            .query_map([&internal_id.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| format!("查询属性失败: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取属性失败: {}", e))?;
        
        for (key, value, value_type) in attrs {
            let json_value = match value_type.as_str() {
                "null" => serde_json::Value::Null,
                "boolean" => serde_json::Value::Bool(value.parse().unwrap_or(false)),
                "number" => serde_json::Value::Number(
                    serde_json::Number::from_f64(value.parse().unwrap_or(0.0))
                        .unwrap_or_else(|| 0.into())
                ),
                "json" => serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value)),
                _ => serde_json::Value::String(value),
            };
            record.data.insert(key, json_value);
        }
        
        records.push(record);
    }
    
    Ok(CommandResponse::success(records))
}

/// 删除记录
#[tauri::command]
pub fn sqlite_delete(
    pwa_id: String,
    db_name: String,
    table_name: String,
    data_id: String,
) -> Result<CommandResponse<bool>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    
    let conn = get_db_conn()?;
    conn.execute(
        "DELETE FROM pwa_data 
         WHERE pwa_id = ?1 AND db_name = ?2 AND table_name = ?3 AND data_id = ?4",
        [&pwa_id, &db_name, &table_name, &data_id],
    ).map_err(|e| format!("删除失败: {}", e))?;
    
    Ok(CommandResponse::success(true))
}

/// 查询单条记录
#[tauri::command]
pub fn sqlite_find_one(
    pwa_id: String,
    db_name: String,
    table_name: String,
    data_id: String,
) -> Result<CommandResponse<Option<Record>>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    
    // 直接查询而不是递归调用
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    
    let conn = get_db_conn()?;
    
    // 查询主表
    let row: Option<(i64, i64, i64)> = conn.query_row(
        "SELECT id, created_at, updated_at FROM pwa_data 
         WHERE pwa_id = ?1 AND db_name = ?2 AND table_name = ?3 AND data_id = ?4",
        [&pwa_id, &db_name, &table_name, &data_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(|e| format!("查询失败: {}", e))?;
    
    let (internal_id, created_at, updated_at) = match row {
        Some(r) => r,
        None => return Ok(CommandResponse::success(None)),
    };
    
    // 查询属性
    let mut record = Record {
        data_id: data_id.clone(),
        created_at,
        updated_at,
        data: HashMap::new(),
    };
    
    let mut stmt = conn.prepare(
        "SELECT key, value, value_type FROM pwa_schema_data WHERE data_id = ?1"
    ).map_err(|e| format!("准备属性查询失败: {}", e))?;
    
    let attrs: Vec<(String, String, String)> = stmt
        .query_map([&internal_id.to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| format!("查询属性失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取属性失败: {}", e))?;
    
    for (key, value, value_type) in attrs {
        let json_value = match value_type.as_str() {
            "null" => serde_json::Value::Null,
            "boolean" => serde_json::Value::Bool(value.parse().unwrap_or(false)),
            "number" => serde_json::Value::Number(
                serde_json::Number::from_f64(value.parse().unwrap_or(0.0))
                    .unwrap_or_else(|| 0.into())
            ),
            "json" => serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value)),
            _ => serde_json::Value::String(value),
        };
        record.data.insert(key, json_value);
    }
    
    Ok(CommandResponse::success(Some(record)))
}

/// 获取记录数
#[tauri::command]
pub fn sqlite_count(
    pwa_id: String,
    db_name: String,
    table_name: String,
    filter: Option<HashMap<String, serde_json::Value>>,
) -> Result<CommandResponse<usize>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    
    let conn = get_db_conn()?;
    
    let mut sql = format!(
        "SELECT COUNT(*) FROM pwa_data d 
         WHERE d.pwa_id = '{}' AND d.db_name = '{}' AND d.table_name = '{}'",
        pwa_id, db_name, table_name
    );
    
    if let Some(f) = filter {
        for (key, value) in f {
            let value_str = json_to_string(&value);
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM pwa_schema_data s 
                   WHERE s.data_id = d.id AND s.key = '{}' AND s.value = '{}')",
                key, value_str
            ));
        }
    }
    
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| format!("计数失败: {}", e))?;
    
    Ok(CommandResponse::success(count as usize))
}

/// 清空表
#[tauri::command]
pub fn sqlite_clear_table(
    pwa_id: String,
    db_name: String,
    table_name: String,
) -> Result<CommandResponse<bool>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    let db_name = sanitize_id(&db_name)?;
    let table_name = sanitize_id(&table_name)?;
    
    let conn = get_db_conn()?;
    conn.execute(
        "DELETE FROM pwa_data 
         WHERE pwa_id = ?1 AND db_name = ?2 AND table_name = ?3",
        [&pwa_id, &db_name, &table_name],
    ).map_err(|e| format!("清空表失败: {}", e))?;
    
    Ok(CommandResponse::success(true))
}

/// 列出所有表
#[tauri::command]
pub fn sqlite_list_tables(
    pwa_id: String,
    db_name: Option<String>,
) -> Result<CommandResponse<Vec<String>>, String> {
    let pwa_id = sanitize_id(&pwa_id)?;
    
    let conn = get_db_conn()?;
    
    let sql = if let Some(db) = db_name {
        format!(
            "SELECT DISTINCT table_name FROM pwa_data 
             WHERE pwa_id = '{}' AND db_name = '{}' ORDER BY table_name",
            pwa_id, sanitize_id(&db)?
        )
    } else {
        format!(
            "SELECT DISTINCT table_name FROM pwa_data 
             WHERE pwa_id = '{}' ORDER BY table_name",
            pwa_id
        )
    };
    
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备查询失败: {}", e))?;
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("查询失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取失败: {}", e))?;
    
    Ok(CommandResponse::success(tables))
}

// ==================== 辅助函数和类型 ====================

fn json_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub data_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct QueryOptions {
    pub order_by: Option<String>,
    pub desc: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 统计数据结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqliteStats {
    pub entries: u64,
    pub total_size_bytes: u64,
    pub total_size_kb: f64,
}

// EAV 模型实现完成
