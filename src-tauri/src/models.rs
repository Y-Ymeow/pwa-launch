use serde::{Deserialize, Serialize};

/// PWA 应用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon_url: Option<String>,
    pub manifest_url: Option<String>,
    pub installed_at: i64,
    pub updated_at: i64,
    pub start_url: Option<String>,
    pub scope: Option<String>,
    pub theme_color: Option<String>,
    pub background_color: Option<String>,
    pub display_mode: String,
}

/// 安装请求
#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub url: String,
    pub name: Option<String>,
}

/// 备份信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub backup_path: String,
    pub created_at: i64,
    pub size_bytes: Option<u64>,
}

/// 应用列表响应
#[derive(Debug, Serialize, Deserialize)]
pub struct AppListResponse {
    pub apps: Vec<AppInfo>,
    pub total: usize,
}

/// 操作响应
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> CommandResponse<T> {
    pub fn success(data: T) -> Self {
        CommandResponse {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        CommandResponse {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

/// 快捷方式信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutInfo {
    pub app_id: String,
    pub shortcut_path: String,
    pub platform: String,
}
