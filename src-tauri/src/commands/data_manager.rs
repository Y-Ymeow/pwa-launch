//! 数据管理 - 查看和清理应用数据

use std::path::Path;
use tauri::Manager;

use crate::models::CommandResponse;

/// 数据使用统计
#[derive(Debug, serde::Serialize)]
pub struct DataUsageInfo {
    pub app_id: String,
    pub app_name: String,
    pub total_bytes: u64,
    pub sqlite_bytes: u64,
    pub cache_bytes: u64,
    pub persistent_cache_bytes: u64,
    pub opfs_bytes: u64,
    pub file_count: usize,
}

/// 系统数据使用统计
#[derive(Debug, serde::Serialize)]
pub struct SystemDataInfo {
    pub database_bytes: u64,
    pub total_cache_bytes: u64,
    pub total_pwa_data_bytes: u64,
    pub webview_cache_bytes: u64,
}

/// 计算目录大小
fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let mut size = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    size += meta.len();
                }
            } else if path.is_dir() {
                size += dir_size(&path);
            }
        }
    }
    size
}

/// 计算文件数量
fn file_count(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }

    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += file_count(&path);
            }
        }
    }
    count
}

/// 获取数据库中某个 app 的数据大小（估算）
fn get_db_data_size_for_app(app_id: &str) -> Result<(u64, u64, u64), String> {
    let conn = crate::DB_CONN.get()
        .ok_or("DB not initialized")?
        .lock()
        .map_err(|e| e.to_string())?;

    // KV store 数据大小（估算：key + value 长度）
    let kv_size: i64 = conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(key) + LENGTH(value)), 0) FROM kv_store WHERE app_id = ?",
        [app_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    // EAV 数据大小（pwa_data 表）
    let eav_data_size: i64 = conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(data_id) + LENGTH(pwa_id) + LENGTH(db_name) + LENGTH(table_name)), 0) 
         FROM pwa_data WHERE pwa_id = ?",
        [app_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    // EAV schema 数据大小
    let eav_schema_size: i64 = conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(key) + LENGTH(value)), 0) 
         FROM pwa_schema_data 
         WHERE data_id IN (SELECT id FROM pwa_data WHERE pwa_id = ?)",
        [app_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    Ok((kv_size as u64, eav_data_size as u64, eav_schema_size as u64))
}

/// 从数据库获取应用名称
fn get_app_name_from_db(app_id: &str) -> Option<String> {
    let conn = crate::DB_CONN.get()?.lock().ok()?;
    conn.query_row(
        "SELECT name FROM apps WHERE id = ?",
        [app_id],
        |row| row.get(0),
    ).ok()
}

/// 获取数据使用统计
#[tauri::command]
pub async fn get_data_usage(
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<DataUsageInfo>>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;

    let pwa_data_dir = app_data_dir.join("pwa_data");
    let mut results = Vec::new();

    // 收集所有 app_id（包括文件目录和数据库中的）
    let mut app_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 从文件目录收集
    if let Ok(entries) = std::fs::read_dir(&pwa_data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(app_id) = path.file_name().and_then(|n| n.to_str()) {
                    app_ids.insert(app_id.to_string());
                }
            }
        }
    }

    // 从数据库 kv_store 收集
    if let Ok(conn) = crate::DB_CONN.get().ok_or("DB not initialized")?.lock() {
        let mut stmt = conn.prepare("SELECT DISTINCT app_id FROM kv_store")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| e.to_string())?;
        for app_id in rows.flatten() {
            app_ids.insert(app_id);
        }
    }

    // 从数据库 pwa_data 收集
    if let Ok(conn) = crate::DB_CONN.get().ok_or("DB not initialized")?.lock() {
        let mut stmt = conn.prepare("SELECT DISTINCT pwa_id FROM pwa_data")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| e.to_string())?;
        for app_id in rows.flatten() {
            app_ids.insert(app_id);
        }
    }

    // 计算每个 app 的数据大小
    for app_id in app_ids {
        let path = pwa_data_dir.join(&app_id);

        // 文件数据大小
        let file_bytes = dir_size(&path);
        let sqlite_file_bytes = dir_size(&path.join("sqlite"));
        let cache_bytes = dir_size(&path.join("cache"));
        let persistent_cache_bytes = dir_size(&path.join("persistent_cache"));
        let opfs_bytes = dir_size(&path.join("opfs"));
        let file_count_val = file_count(&path);

        // 数据库中的数据大小
        let (kv_db_bytes, eav_data_bytes, eav_schema_bytes) = 
            get_db_data_size_for_app(&app_id).unwrap_or((0, 0, 0));

        // 总数据库数据大小（文件 + 数据库表）
        let sqlite_bytes = sqlite_file_bytes + kv_db_bytes + eav_data_bytes + eav_schema_bytes;

        let total_bytes = file_bytes + kv_db_bytes + eav_data_bytes + eav_schema_bytes;

        // 获取应用名称
        let app_name = get_app_name_from_db(&app_id)
            .unwrap_or_else(|| app_id.clone());

        results.push(DataUsageInfo {
            app_id: app_id.clone(),
            app_name,
            total_bytes,
            sqlite_bytes,
            cache_bytes,
            persistent_cache_bytes,
            opfs_bytes,
            file_count: file_count_val,
        });
    }

    // 按大小排序
    results.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    Ok(CommandResponse::success(results))
}

/// 获取系统数据使用统计
#[tauri::command]
pub async fn get_system_data_info(
    app: tauri::AppHandle,
) -> Result<CommandResponse<SystemDataInfo>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;

    // 数据库大小
    let db_path = app_data_dir.join("pwa_container.db");
    let database_bytes = if db_path.exists() {
        std::fs::metadata(&db_path)
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    // PWA 数据总大小
    let pwa_data_dir = app_data_dir.join("pwa_data");
    let total_pwa_data_bytes = dir_size(&pwa_data_dir);

    // 网络缓存总大小
    let pwa_cache_dir = app_data_dir.join("pwa_cache");
    let total_cache_bytes = dir_size(&pwa_cache_dir);

    // WebView 缓存
    let cache_dir = app.path()
        .cache_dir()
        .map_err(|e| format!("获取缓存目录失败: {}", e))?;
    let webview_cache = cache_dir.join("WebView");
    let webview_cache_bytes = dir_size(&webview_cache);

    Ok(CommandResponse::success(SystemDataInfo {
        database_bytes,
        total_cache_bytes,
        total_pwa_data_bytes,
        webview_cache_bytes,
    }))
}

/// 清理指定应用的数据
#[tauri::command]
pub async fn clear_app_data(
    app_id: String,
    clear_sqlite: bool,
    clear_cache: bool,
    clear_persistent_cache: bool,
    clear_opfs: bool,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;

    let app_dir = app_data_dir.join("pwa_data").join(&app_id);

    // 清理文件数据
    if clear_sqlite {
        let sqlite_dir = app_dir.join("sqlite");
        if sqlite_dir.exists() {
            let _ = std::fs::remove_dir_all(&sqlite_dir);
            log::info!("[DataManager] 已清理 SQLite 文件数据: {}", app_id);
        }

        // 同时清理数据库中的 KV 和 EAV 数据
        if let Ok(conn) = crate::DB_CONN.get().ok_or("DB not initialized")?.lock() {
            // 启用外键约束（确保级联删除生效）
            if let Err(e) = conn.execute("PRAGMA foreign_keys = ON", []) {
                log::warn!("[DataManager] 启用外键约束失败: {}", e);
            }

            // 开始事务
            if let Err(e) = conn.execute("BEGIN TRANSACTION", []) {
                log::error!("[DataManager] 开始事务失败: {}", e);
            }

            // 1. 清理 KV store
            match conn.execute("DELETE FROM kv_store WHERE app_id = ?", [&app_id]) {
                Ok(deleted) => {
                    log::info!("[DataManager] 已清理 KV store 数据: {} 行, app_id: {}", deleted, app_id);
                }
                Err(e) => {
                    log::error!("[DataManager] 清理 KV store 失败: {}", e);
                }
            }

            // 2. 手动清理 pwa_schema_data（通过 data_id 关联）
            // 先获取所有要删除的 data_id
            let data_ids: Vec<i64> = match conn.prepare("SELECT id FROM pwa_data WHERE pwa_id = ?") {
                Ok(mut stmt) => {
                    match stmt.query_map([&app_id], |row| row.get(0)) {
                        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                        Err(e) => {
                            log::error!("[DataManager] 查询 data_ids 失败: {}", e);
                            vec![]
                        }
                    }
                }
                Err(e) => {
                    log::error!("[DataManager] 准备查询失败: {}", e);
                    vec![]
                }
            };

            // 删除 pwa_schema_data
            for data_id in &data_ids {
                if let Err(e) = conn.execute("DELETE FROM pwa_schema_data WHERE data_id = ?", [data_id]) {
                    log::error!("[DataManager] 清理 pwa_schema_data 失败, data_id={}: {}", data_id, e);
                }
            }
            log::info!("[DataManager] 已清理 pwa_schema_data: {} 行, pwa_id: {}", data_ids.len(), app_id);

            // 3. 清理 pwa_data
            match conn.execute("DELETE FROM pwa_data WHERE pwa_id = ?", [&app_id]) {
                Ok(deleted) => {
                    log::info!("[DataManager] 已清理 pwa_data: {} 行, pwa_id: {}", deleted, app_id);
                }
                Err(e) => {
                    log::error!("[DataManager] 清理 pwa_data 失败: {}", e);
                }
            }

            // 提交事务
            if let Err(e) = conn.execute("COMMIT", []) {
                log::error!("[DataManager] 提交事务失败: {}", e);
                // 尝试回滚
                let _ = conn.execute("ROLLBACK", []);
            }
        }
    }

    if clear_cache {
        let cache_dir = app_dir.join("cache");
        if cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&cache_dir);
            log::info!("[DataManager] 已清理缓存: {}", app_id);
        }
    }

    if clear_persistent_cache {
        let persistent_cache_dir = app_dir.join("persistent_cache");
        if persistent_cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&persistent_cache_dir);
            log::info!("[DataManager] 已清理持久化缓存: {}", app_id);
        }
    }

    if clear_opfs {
        let opfs_dir = app_dir.join("opfs");
        if opfs_dir.exists() {
            let _ = std::fs::remove_dir_all(&opfs_dir);
            log::info!("[DataManager] 已清理 OPFS 数据: {}", app_id);
        }
    }

    Ok(CommandResponse::success(true))
}

/// 清理所有网络缓存
#[tauri::command]
pub async fn clear_all_network_cache(
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;

    let pwa_cache_dir = app_data_dir.join("pwa_cache");
    if pwa_cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&pwa_cache_dir);
        log::info!("[DataManager] 已清理所有网络缓存");
    }

    Ok(CommandResponse::success(true))
}

/// 清理 WebView 缓存
#[tauri::command]
pub async fn clear_webview_cache_data(
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let cache_dir = app.path()
        .cache_dir()
        .map_err(|e| format!("获取缓存目录失败: {}", e))?;

    let webview_cache = cache_dir.join("WebView");
    if webview_cache.exists() {
        let _ = std::fs::remove_dir_all(&webview_cache);
        log::info!("[DataManager] 已清理 WebView 缓存");
    }

    let default_cache = cache_dir.join("Default");
    if default_cache.exists() {
        let _ = std::fs::remove_dir_all(&default_cache);
        log::info!("[DataManager] 已清理 Default 缓存");
    }

    Ok(CommandResponse::success(true))
}

/// 格式化字节大小
#[tauri::command]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let exp = (bytes as f64).log(1024.0).min(UNITS.len() as f64 - 1.0) as usize;
    let value = bytes as f64 / 1024f64.powi(exp as i32);

    if exp == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", value, UNITS[exp])
    }
}
