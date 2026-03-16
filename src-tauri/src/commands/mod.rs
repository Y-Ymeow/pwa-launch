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
    pub enabled: bool,
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Http,
    Https,
    Socks5,
}

impl Default for ProxyType {
    fn default() -> Self {
        ProxyType::Http
    }
}

impl ProxySettings {
    /// 生成代理 URL
    pub fn get_proxy_url(&self) -> String {
        let scheme = match self.proxy_type {
            ProxyType::Http => "http",
            ProxyType::Https => "https",
            ProxyType::Socks5 => "socks5",
        };

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            format!("{}://{}:{}@{}:{}", scheme, user, pass, self.host, self.port)
        } else {
            format!("{}://{}:{}", scheme, self.host, self.port)
        }
    }
}

// 子模块
pub mod backup;
pub mod cookie;
pub mod file_dialog;
pub mod fs;
pub mod opfs;
pub mod proxy;
pub mod screen;

pub mod pwa;

pub mod webview;

// 重新导出
pub use backup::*;
pub use cookie::*;
pub use file_dialog::*;
pub use fs::*;
pub use opfs::*;
pub use proxy::*;
pub use screen::*;

pub use pwa::*;
pub use webview::*;

// 辅助函数
fn extract_domain(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().unwrap_or("").to_string()
    } else {
        url.to_string()
    }
}
