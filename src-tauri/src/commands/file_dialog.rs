use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_fs::FsExt;

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
                        // Android 上使用 URL (content://)，桌面端使用路径
                        #[cfg(target_os = "android")]
                        {
                            let url = fp.url().to_string();
                            log::info!("  - URL: {}", url);
                            Some(url)
                        }
                        #[cfg(not(target_os = "android"))]
                        {
                            match fp.into_path() {
                                Ok(path) => {
                                    let path_str = path.to_string_lossy().to_string();
                                    log::info!("  - Path: {}", path_str);
                                    Some(path_str)
                                }
                                Err(e) => {
                                    log::error!("Failed to convert FilePath to PathBuf: {:?}", e);
                                    None
                                }
                            }
                        }
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
                #[cfg(target_os = "android")]
                {
                    let url = file_path.url().to_string();
                    log::info!("Selected file URL: {}", url);
                    vec![url]
                }
                #[cfg(not(target_os = "android"))]
                {
                    match file_path.into_path() {
                        Ok(path) => {
                            let path_str = path.to_string_lossy().to_string();
                            log::info!("Selected file: {}", path_str);
                            vec![path_str]
                        }
                        Err(e) => {
                            log::error!("Failed to convert FilePath to PathBuf: {:?}", e);
                            vec![]
                        }
                    }
                }
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

#[tauri::command]
pub async fn read_file_content(
    app: tauri::AppHandle,
    path: String,
) -> Result<CommandResponse<serde_json::Value>, String> {
    log::info!("read_file_content: {}", path);

    let (content, name, size, mime_type): (Vec<u8>, String, u64, String);

    #[cfg(target_os = "android")]
    {
        // Android: 使用 tauri_plugin_fs 读取 content:// URI
        if path.starts_with("content://") {
            use tauri_plugin_fs::FsExt;
            let fs_ext = app.fs();
            content = fs_ext
                .read(path.clone())
                .map_err(|e| format!("读取文件失败：{}", e))?;
            
            // 从 URL 提取文件名
            name = path
                .split('/')
                .last()
                .unwrap_or("unknown")
                .to_string();
            size = content.len() as u64;
        } else {
            // 普通路径
            let path_buf = PathBuf::from(&path);
            if !path_buf.exists() {
                return Err("文件不存在".to_string());
            }
            content = std::fs::read(&path_buf).map_err(|e| format!("读取文件失败：{}", e))?;
            name = path_buf
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            size = content.len() as u64;
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        // Linux/Desktop: 直接使用 std::fs
        let path_buf = PathBuf::from(&path);
        if !path_buf.exists() {
            return Err("文件不存在".to_string());
        }
        let metadata = std::fs::metadata(&path_buf)
            .map_err(|e| format!("读取文件信息失败：{}", e))?;
        content = std::fs::read(&path_buf).map_err(|e| format!("读取文件失败：{}", e))?;
        name = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        size = metadata.len();
    }

    use base64::Engine;
    let base64_content = base64::engine::general_purpose::STANDARD.encode(&content);

    // 从文件名或路径提取扩展名
    let ext = name
        .split('.')
        .last()
        .unwrap_or("")
        .to_lowercase();

    mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wma" => "audio/x-ms-wma",
        "lrc" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string();

    log::info!("read_file_content: {} ({} bytes)", name, size);

    Ok(CommandResponse::success(serde_json::json!({
        "name": name,
        "path": path,
        "size": size,
        "mimeType": mime_type,
        "content": base64_content,
    })))
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

/// 读取文件指定范围的内容（用于获取元数据）
#[tauri::command]
pub async fn read_file_range(
    app: tauri::AppHandle,
    path: String,
    offset: u64,
    length: u64,
) -> Result<CommandResponse<serde_json::Value>, String> {
    log::info!("read_file_range: {} offset={} length={}", path, offset, length);

    #[cfg(target_os = "android")]
    {
        // Android: content:// URI 不支持随机访问，需要读取整个文件
        if path.starts_with("content://") {
            use tauri_plugin_fs::FsExt;
            let fs_ext = app.fs();
            let full_content = fs_ext
                .read(path.clone())
                .map_err(|e| format!("读取文件失败：{}", e))?;

            let file_size = full_content.len() as u64;
            let actual_offset = offset.min(file_size);
            let max_length = file_size - actual_offset;
            let actual_length = length.min(max_length).min(10 * 1024 * 1024) as usize;

            let end = (actual_offset as usize + actual_length).min(full_content.len());
            let buffer = full_content[actual_offset as usize..end].to_vec();

            use base64::Engine;
            let base64_content = base64::engine::general_purpose::STANDARD.encode(&buffer);

            let name = path
                .split('/')
                .last()
                .unwrap_or("unknown")
                .to_string();

            log::info!("read_file_range: {} read {} bytes", name, buffer.len());

            return Ok(CommandResponse::success(serde_json::json!({
                "name": name,
                "path": path,
                "size": file_size,
                "offset": actual_offset,
                "length": buffer.len(),
                "content": base64_content,
            })));
        }
    }

    // Desktop: 使用标准文件操作
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err("文件不存在".to_string());
    }

    let mut file = File::open(&path_buf).map_err(|e| format!("打开文件失败：{}", e))?;
    let file_size = file
        .metadata()
        .map_err(|e| format!("获取文件信息失败：{}", e))?
        .len();

    let actual_offset = offset.min(file_size);
    let max_length = file_size - actual_offset;
    let actual_length = length.min(max_length).min(10 * 1024 * 1024);

    file.seek(SeekFrom::Start(actual_offset))
        .map_err(|e| format!("定位文件失败：{}", e))?;

    let mut buffer = vec![0u8; actual_length as usize];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| format!("读取文件失败：{}", e))?;
    buffer.truncate(bytes_read);

    use base64::Engine;
    let base64_content = base64::engine::general_purpose::STANDARD.encode(&buffer);

    let name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    log::info!("read_file_range: {} read {} bytes", name, bytes_read);

    Ok(CommandResponse::success(serde_json::json!({
        "name": name,
        "path": path,
        "size": file_size,
        "offset": actual_offset,
        "length": bytes_read,
        "content": base64_content,
    })))
}