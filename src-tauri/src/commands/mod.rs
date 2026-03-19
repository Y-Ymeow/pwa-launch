use crate::models::CommandResponse;

// 子模块
pub mod cookie;
pub mod data_manager;
pub mod file_dialog;
pub mod fs;
pub mod pwa;
pub mod screen;
pub mod webview;

// 重新导出
pub use cookie::*;
pub use data_manager::*;
pub use file_dialog::*;
pub use fs::*;
pub use file_dialog::read_file_content;
pub use file_dialog::read_file_range;
pub use pwa::*;
pub use screen::*;
pub use webview::*;

// 辅助函数：提取域名
fn extract_domain(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().unwrap_or("").to_string()
    } else {
        url.to_string()
    }
}
