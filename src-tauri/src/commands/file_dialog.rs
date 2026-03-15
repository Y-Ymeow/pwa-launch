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
                        // 尝试获取路径
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
pub async fn read_file_content(path: String) -> Result<CommandResponse<serde_json::Value>, String> {
    use std::fs;

    let path_buf = PathBuf::from(&path);

    log::info!("read_file_content: {}", path);

    if !path_buf.exists() {
        return Err("文件不存在".to_string());
    }

    let metadata = fs::metadata(&path_buf).map_err(|e| format!("读取文件信息失败：{}", e))?;

    let name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let content = fs::read(&path_buf).map_err(|e| format!("读取文件失败：{}", e))?;

    use base64::Engine;
    let base64_content = base64::engine::general_purpose::STANDARD.encode(&content);

    let ext = path_buf
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wma" => "audio/x-ms-wma",
        "lrc" => "text/plain",
        _ => "application/octet-stream",
    };

    log::info!("read_file_content: {} ({} bytes)", name, metadata.len());

    Ok(CommandResponse::success(serde_json::json!({
        "name": name,
        "path": path,
        "size": metadata.len(),
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
    path: String,
    offset: u64,
    length: u64,
) -> Result<CommandResponse<serde_json::Value>, String> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let path_buf = PathBuf::from(&path);

    log::info!(
        "read_file_range: {} offset={} length={}",
        path,
        offset,
        length
    );

    if !path_buf.exists() {
        return Err("文件不存在".to_string());
    }

    let mut file = File::open(&path_buf).map_err(|e| format!("打开文件失败：{}", e))?;

    // 获取文件大小
    let file_size = file
        .metadata()
        .map_err(|e| format!("获取文件信息失败：{}", e))?
        .len();

    // 限制读取范围
    let actual_offset = offset.min(file_size);
    let max_length = file_size - actual_offset;
    let actual_length = length.min(max_length).min(10 * 1024 * 1024); // 最大 10MB

    // 定位并读取
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