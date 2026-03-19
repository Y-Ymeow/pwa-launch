use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_fs::FsExt;
use base64::Engine as _;

use super::CommandResponse;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenDialogOptions {
    pub title: Option<String>,
    pub multiple: Option<bool>,
    pub filters: Option<Vec<FileFilter>>,
    pub directory: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenDialogResponse {
    pub paths: Vec<String>,
}

#[tauri::command]
pub async fn open_file_dialog(
    app: tauri::AppHandle,
    title: Option<String>,
    multiple: Option<bool>,
    filters: Option<Vec<FileFilter>>,
    directory: Option<bool>,
) -> Result<CommandResponse<OpenDialogResponse>, String> {
    use tauri_plugin_dialog::DialogExt;

    log::info!(
        "open_file_dialog called: title={:?}, multiple={:?}, filters={:?}, directory={:?}",
        title, multiple, filters, directory
    );

    // 使用 dialog 插件打开文件选择器
    let mut dialog = app.dialog().file();

    // 设置标题
    if let Some(title) = title {
        dialog = dialog.set_title(title);
    }

    // 添加文件过滤器
    if let Some(filters) = filters {
        for filter in filters {
            // 去掉扩展名前面的点号
            let exts: Vec<&str> = filter
                .extensions
                .iter()
                .map(|e| e.trim_start_matches('.'))
                .collect();
            if !exts.is_empty() {
                dialog = dialog.add_filter(filter.name, exts.as_slice());
            }
        }
    }

    // 执行选择
    let paths: Vec<String> = if multiple.unwrap_or(false) {
        match dialog.blocking_pick_files() {
            Some(file_paths) => {
                log::info!("Selected {} files", file_paths.len());
                file_paths
                    .into_iter()
                    .filter_map(|fp| {
                        // 直接转换为字符串 (Android 返回 content:// URI，桌面返回路径)
                        let path_str = fp.to_string();
                        log::info!("  - Path: {}", path_str);
                        // Android: 直接返回 content:// URI，播放器会处理权限
                        Some(path_str)
                    })
                    .collect()
            }
            None => {
                log::info!("No files selected");
                vec![]
            }
        }
    } else {
        match dialog.blocking_pick_file() {
            Some(file_path) => {
                let path_str = file_path.to_string();
                log::info!("Selected file: {}", path_str);
                // Android: 直接返回 content:// URI，播放器会处理权限
                vec![path_str]
            }
            None => {
                log::info!("No file selected");
                vec![]
            }
        }
    };

    log::info!("Returning {} paths", paths.len());
    Ok(CommandResponse::success(OpenDialogResponse { paths }))
}

/// Android: 将 content:// URI 复制到应用私有目录
#[cfg(target_os = "android")]
fn copy_content_uri_to_cache(app: &tauri::AppHandle, uri_str: &str) -> Result<String, String> {
    use tauri_plugin_fs::FsExt;
    use std::io::Write;
    
    log::info!("Copying content URI to cache: {}", uri_str);
    
    // 解析文件名
    let file_name = uri_str
        .split('/')
        .last()
        .unwrap_or("unknown_file")
        .split(':')
        .last()
        .unwrap_or("unknown_file");
    
    // 读取 content URI 内容
    let fs_ext = app.fs();
    let url = tauri::Url::parse(uri_str)
        .map_err(|e| format!("无效的 URI: {}", e))?;
    let content = fs_ext
        .read(url)
        .map_err(|e| format!("读取 content URI 失败: {}", e))?;
    
    // 写入应用缓存目录
    let cache_dir = app.path()
        .cache_dir()
        .map_err(|e| format!("获取缓存目录失败: {}", e))?;
    let target_path = cache_dir.join(file_name);
    
    std::fs::write(&target_path, &content)
        .map_err(|e| format!("写入缓存文件失败: {}", e))?;
    
    let result_path = target_path.to_string_lossy().to_string();
    log::info!("Content URI copied to: {}", result_path);
    
    Ok(result_path)
}

#[tauri::command]
pub async fn resolve_local_file_url(path: String) -> Result<CommandResponse<String>, String> {
    log::info!("resolve_local_file_url: {}", path);

    // 返回本地 HTTP 服务器 URL
    let port = crate::local_server::get_local_server_port();
    let encoded_path = urlencoding::encode(&path);
    let url = format!("http://localhost:{}/local/file/{}", port, encoded_path);

    log::info!("Local file URL: {}", url);
    Ok(CommandResponse::success(url))
}

/// 读取文件内容（返回 Base64）
#[tauri::command]
pub async fn read_file_content(path: String) -> Result<CommandResponse<FileContentResponse>, String> {
    use std::fs;
    use std::path::Path;
    
    log::info!("read_file_content: {}", path);
    
    let path_obj = Path::new(&path);
    let name = path_obj.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    // 检测 MIME 类型
    let mime_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    
    // 读取文件内容
    let content = fs::read(&path).map_err(|e| format!("读取文件失败: {}", e))?;
    let size = content.len() as u64;
    
    // Base64 编码
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(&content);
    
    log::info!("read_file_content success: {} bytes", size);
    
    Ok(CommandResponse::success(FileContentResponse {
        name,
        path,
        size,
        mime_type,
        content: content_b64,
    }))
}

/// 读取文件范围（用于大文件分段读取）
#[tauri::command]
pub async fn read_file_range(
    path: String, 
    offset: u64, 
    length: u64
) -> Result<CommandResponse<FileRangeResponse>, String> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::Path;
    
    log::info!("read_file_range: {} offset={} length={}", path, offset, length);
    
    let path_obj = Path::new(&path);
    let name = path_obj.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    // 获取文件大小
    let metadata = std::fs::metadata(&path).map_err(|e| format!("获取文件元数据失败: {}", e))?;
    let file_size = metadata.len();
    
    // 打开文件并定位
    let mut file = File::open(&path).map_err(|e| format!("打开文件失败: {}", e))?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| format!("定位文件失败: {}", e))?;
    
    // 读取指定范围
    let read_length = std::cmp::min(length, file_size - offset);
    let mut buffer = vec![0u8; read_length as usize];
    let bytes_read = file.read(&mut buffer).map_err(|e| format!("读取文件失败: {}", e))?;
    buffer.truncate(bytes_read);
    
    // Base64 编码
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(&buffer);
    
    log::info!("read_file_range success: {} bytes read", bytes_read);
    
    Ok(CommandResponse::success(FileRangeResponse {
        name,
        path: path.clone(),
        size: file_size,
        offset,
        length: bytes_read as u64,
        content: content_b64,
    }))
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct FileContentResponse {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mime_type: String,
    pub content: String, // Base64
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct FileRangeResponse {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub offset: u64,
    pub length: u64,
    pub content: String, // Base64
}