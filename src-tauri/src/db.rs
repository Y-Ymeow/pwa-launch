use std::path::Path;
use rusqlite::{Connection, Result};

/// 获取应用特定的数据目录
pub fn get_app_data_dir(app_id: &str, base_dir: &Path) -> PathBuf {
    base_dir.join("pwa_data").join(app_id)
}

use std::path::PathBuf;

/// 初始化数据库
pub fn init_db(app_data_dir: &Path) -> Result<()> {
    let db_path = app_data_dir.join("pwa_container.db");

    let conn = Connection::open(&db_path)?;

    // 创建应用表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS apps (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            icon_url TEXT,
            manifest_url TEXT,
            installed_at INTEGER,
            updated_at INTEGER,
            start_url TEXT,
            scope TEXT,
            theme_color TEXT,
            background_color TEXT,
            display_mode TEXT
        )",
        [],
    )?;

    // 创建键值对存储表 (持久化 localStorage)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kv_store (
            app_id TEXT,
            key TEXT,
            value TEXT,
            PRIMARY KEY (app_id, key)
        )",
        [],
    )?;

    // 创建备份记录表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS backups (
            id TEXT PRIMARY KEY,
            app_id TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            size INTEGER NOT NULL,
            version TEXT
        )",
        [],
    )?;

    Ok(())
}

pub type DbConnection = std::sync::Mutex<Connection>;
