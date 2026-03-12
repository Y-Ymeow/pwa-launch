use std::path::PathBuf;

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
    options: OpenDialogOptions,
) -> Result<CommandResponse<OpenDialogResponse>, String> {
    use tauri_plugin_dialog::DialogExt;

    log::info!("open_file_dialog called with options: {:?}", options);

    // 使用 dialog 插件打开文件选择器
    let mut dialog = app.dialog().file();

    // 设置标题
    if let Some(title) = options.title {
        dialog = dialog.set_title(title);
    }

    // 添加文件过滤器
    if let Some(filters) = options.filters {
        for filter in filters {
            // 去掉扩展名前面的点号
            let exts: Vec<&str> = filter.extensions
                .iter()
                .map(|e| e.trim_start_matches('.'))
                .collect();
            if !exts.is_empty() {
                dialog = dialog.add_filter(filter.name, exts.as_slice());
            }
        }
    }

    // 执行选择
    let paths: Vec<String> = if options.multiple.unwrap_or(false) {
        match dialog.blocking_pick_files() {
            Some(file_paths) => {
                log::info!("Selected files: {:?}", file_paths);
                file_paths
                    .into_iter()
                    .filter_map(|fp| {
                        let path_str = fp.into_path().ok()?.to_string_lossy().to_string();
                        log::info!("  - {}", path_str);
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
                let path_str = file_path.into_path().ok()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                log::info!("Selected file: {}", path_str);
                if path_str.is_empty() { vec![] } else { vec![path_str] }
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

    // 处理 static://localhost/ 前缀（如果前端传了 URL 而不是路径）
    let path = if path.starts_with("static://localhost/") {
        let encoded = &path["static://localhost/".len()..];
        urlencoding::decode(encoded).unwrap_or_else(|_| encoded.into()).to_string()
    } else if path.starts_with("http://static.localhost/") {
        let encoded = &path["http://static.localhost/".len()..];
        urlencoding::decode(encoded).unwrap_or_else(|_| encoded.into()).to_string()
    } else {
        path
    };

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
    
    // 根据文件类型选择协议
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    // 音视频文件使用 local server（支持 Range 请求，适合流媒体）
    let is_media = matches!(ext.as_str(), "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "wma" | "mp4" | "webm" | "mkv" | "mov" | "avi");
    
    if is_media {
        // 音视频：使用本地 HTTP 服务器
        let path_clone = path.clone();
        log::info!("Local server URL for media: {}", path_clone);
        if let Some(url) = crate::local_server::get_file_url(path_clone) {
            log::info!("Local server URL for media: {}", url);
            Ok(CommandResponse::success(url))
        } else {
            // 服务器未启动，回退到 static 协议
            log::warn!("Local server not available, falling back to static:// for media");
            let encoded_path = urlencoding::encode(&path);
            let url = format!("static://localhost/{}", encoded_path);
            Ok(CommandResponse::success(url))
        }
    } else {
        // 图片、文档等：使用 static 协议（更快，无 HTTP 开销）
        log::info!("Static URL for file: {}", path);
        let encoded_path = urlencoding::encode(&path);
        // Android 使用 http://static.localhost，其他平台使用 static://localhost
        let url = if cfg!(target_os = "android") {
            format!("http://static.localhost/{}", encoded_path)
        } else {
            format!("static://localhost/{}", encoded_path)
        };
        Ok(CommandResponse::success(url))
    }
}
