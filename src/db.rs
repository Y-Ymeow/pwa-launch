use rusqlite::{Connection, Result};
use std::path::Path;
use std::sync::Mutex;

pub type DbConnection = Mutex<Connection>;

/// 初始化数据库
pub fn init_db(app_data_dir: &Path) -> Result<()> {
    let db_path = app_data_dir.join("pwa_container.db");
    
    let conn = Connection::open(&db_path)?;
    
    // 创建应用信息表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS apps (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            icon_url TEXT,
            manifest_url TEXT,
            installed_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            start_url TEXT,
            scope TEXT,
            theme_color TEXT,
            background_color TEXT,
            display_mode TEXT DEFAULT 'standalone'
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
            size_bytes INTEGER,
            FOREIGN KEY (app_id) REFERENCES apps(id)
        )",
        [],
    )?;
    
    log::info!("数据库初始化完成：{:?}", db_path);
    Ok(())
}

/// 获取应用数据目录
pub fn get_app_data_dir(app_id: &str, app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("apps").join(app_id)
}

/// 获取应用数据库路径
pub fn get_app_db_path(app_id: &str, app_data_dir: &Path) -> std::path::PathBuf {
    get_app_data_dir(app_id, app_data_dir).join("data.db")
}

/// 获取应用文件存储目录
pub fn get_app_files_dir(app_id: &str, app_data_dir: &Path) -> std::path::PathBuf {
    get_app_data_dir(app_id, app_data_dir).join("files")
}

/// 获取应用缓存目录
pub fn get_app_cache_dir(app_id: &str, app_data_dir: &Path) -> std::path::PathBuf {
    get_app_data_dir(app_id, app_data_dir).join("cache")
}

/// 获取备份目录
pub fn get_backup_dir(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("backups")
}
