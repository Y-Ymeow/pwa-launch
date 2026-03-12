use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::get_app_data_dir;
use crate::models::CommandResponse;

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
pub mod backup;
pub mod cookie;
pub mod file_dialog;
pub mod opfs;
pub mod proxy;
pub mod pwa;
pub mod static_protocol;
pub mod stream_file_protocol;
pub mod stream_proxy;
pub mod ws_proxy;

// 重新导出
pub use backup::*;
pub use cookie::*;
pub use file_dialog::*;
pub use opfs::*;
pub use proxy::*;
pub use pwa::*;
pub use stream_proxy::*;
pub use ws_proxy::*;

// 辅助函数
fn extract_domain(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().unwrap_or("").to_string()
    } else {
        url.to_string()
    }
}
