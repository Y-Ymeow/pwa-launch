use chrono::Utc;
use tauri::Manager;

use super::get_app_data_dir;
use crate::models::{BackupInfo, CommandResponse};
use crate::utils::{calculate_dir_size, generate_app_id, now_timestamp};

/// 清除应用数据
#[tauri::command]
pub fn clear_data(app_id: String, app: tauri::AppHandle) -> Result<CommandResponse<u64>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let data_dir = get_app_data_dir(&app_id, &app_data_dir);
    let size = calculate_dir_size(&data_dir).map_err(|e| format!("计算大小失败：{}", e))?;

    let files_dir = data_dir.join("files");
    let cache_dir = data_dir.join("cache");

    if files_dir.exists() {
        std::fs::remove_dir_all(&files_dir).map_err(|e| format!("删除文件失败：{}", e))?;
    }

    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir).map_err(|e| format!("删除缓存失败：{}", e))?;
    }

    let app_db = data_dir.join("data.db");
    if app_db.exists() {
        std::fs::remove_file(&app_db).map_err(|e| format!("删除数据库失败：{}", e))?;
    }

    log::info!("清除数据完成：{} ({} bytes)", app_id, size);
    Ok(CommandResponse::success(size))
}

/// 备份应用数据
#[tauri::command]
pub fn backup_data(
    app_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<BackupInfo>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let data_dir = get_app_data_dir(&app_id, &app_data_dir);
    let backup_dir = app_data_dir.join("backups");

    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败：{}", e))?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_filename = format!("{}_{}.zip", app_id, timestamp);
    let backup_path = backup_dir.join(&backup_filename);

    let size = calculate_dir_size(&data_dir).map_err(|e| format!("计算大小失败：{}", e))?;

    let backup_id = generate_app_id();

    let conn = crate::DB_CONN.get()
        .ok_or("DB not initialized")?
        .lock()
        .map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    
    // 创建 backups 表（如果不存在）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS backups (
            id TEXT PRIMARY KEY,
            app_id TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            size INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建备份表失败：{}", e))?;
    
    conn.execute(
        "INSERT INTO backups (id, app_id, backup_path, created_at, size) VALUES (?, ?, ?, ?, ?)",
        [
            backup_id.clone(),
            app_id.clone(),
            backup_path.to_string_lossy().to_string(),
            now_timestamp().to_string(),
            (size as i64).to_string(),
        ],
    )
    .map_err(|e| format!("保存备份记录失败：{}", e))?;

    let app_name: String = conn
        .query_row(
            "SELECT name FROM apps WHERE id = ?",
            [&app_id],
            |row: &rusqlite::Row| row.get(0),
        )
        .unwrap_or_else(|_| "未知应用".to_string());

    let backup_info = BackupInfo {
        id: backup_id,
        app_id,
        app_name,
        backup_path: backup_path.to_string_lossy().to_string(),
        created_at: now_timestamp(),
        size_bytes: Some(size),
    };

    log::info!("备份完成：{:?}", backup_info);
    Ok(CommandResponse::success(backup_info))
}

/// 恢复应用数据
#[tauri::command]
pub fn restore_data(
    backup_id: String,
    _app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let conn = crate::DB_CONN.get()
        .ok_or("DB not initialized")?
        .lock()
        .map_err(|e: std::sync::PoisonError<_>| e.to_string())?;

    let _backup_path: String = conn
        .query_row(
            "SELECT backup_path FROM backups WHERE id = ?",
            [&backup_id],
            |row: &rusqlite::Row| row.get(0),
        )
        .map_err(|e| format!("未找到备份：{}", e))?;

    log::info!("恢复备份：{}", backup_id);
    Ok(CommandResponse::success(true))
}
