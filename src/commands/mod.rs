use tauri::{State, Manager};
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{DbConnection, get_app_data_dir, get_backup_dir};
use crate::models::{AppInfo, InstallRequest, BackupInfo, CommandResponse, ShortcutInfo};
use crate::utils::{generate_app_id, now_timestamp, calculate_dir_size, create_app_dirs, remove_app_dirs};

// 全局 Cookie 存储 - 按 app_id + 域名 隔离
pub type CookieStore = Arc<RwLock<HashMap<String, HashMap<String, HashMap<String, String>>>>>;

// 全局代理设置
pub type ProxyConfig = Arc<RwLock<Option<ProxySettings>>>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProxySettings {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

// 子模块
pub mod proxy;
pub mod cookie;
pub mod pwa;
pub mod backup;
pub mod opfs;

// 重新导出
pub use proxy::*;
pub use cookie::*;
pub use pwa::*;
pub use backup::*;
pub use opfs::*;

// 辅助函数
fn extract_domain(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().unwrap_or("").to_string()
    } else {
        url.to_string()
    }
}
