use http::{header::*, Response, StatusCode};
use std::io::{Read, Seek, SeekFrom};

/// 处理流式文件请求，支持 Range 和 HEAD
pub fn handle_stream_request(
    request: http::Request<Vec<u8>>,
) -> Result<http::Response<Vec<u8>>, Box<dyn std::error::Error>> {
    let method = request.method();
    let path = request.uri().path();

    // URL 解码
    let decoded_path = match urlencoding::decode(path) {
        Ok(p) => p.to_string(),
        Err(_) => path.to_string(),
    };

    // 去掉开头的 /
    let file_path = if decoded_path.starts_with('/') {
        decoded_path[1..].to_string()
    } else {
        decoded_path
    };

    // 处理 Windows 路径 (例如 /C:/Users -> C:/Users)
    let file_path = if file_path.len() > 2 && file_path.as_bytes()[0] == b'/' && file_path.as_bytes()[2] == b':' {
        file_path[1..].to_string()
    } else {
        file_path
    };

    // 打开文件
    let mut file = match std::fs::File::open(&file_path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("[stream] Failed to open {}: {}", file_path, e);
            return Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new())?);
        }
    };

    let file_size = file.metadata()?.len();
    let ext = std::path::Path::new(&file_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };

    let response_builder = Response::builder()
        .header(CONTENT_TYPE, mime_type)
        .header(ACCEPT_RANGES, "bytes")
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(ACCESS_CONTROL_EXPOSE_HEADERS, "Content-Range, Content-Length, Accept-Ranges");

    // 1. 处理 HEAD 请求 (WebKit 必须要求)
    if method == http::Method::HEAD {
        log::info!("[stream] HEAD request for: {}", file_path);
        return Ok(response_builder
            .header(CONTENT_LENGTH, file_size)
            .body(Vec::new())?);
    }

    // 2. 检查 Range 请求头
    let range_header = request.headers().get(RANGE).and_then(|v| v.to_str().ok());
    
    if let Some(range_str) = range_header {
        if let Some(range_val) = range_str.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_val.split('-').collect();
            if parts.len() == 2 {
                let start: u64 = parts[0].parse().unwrap_or(0);
                let end: u64 = if parts[1].is_empty() {
                    file_size - 1
                } else {
                    parts[1].parse().unwrap_or(file_size - 1)
                };
                
                let end = end.min(file_size - 1);

                if start <= end && start < file_size {
                    let length = end - start + 1;
                    file.seek(SeekFrom::Start(start))?;
                    
                    // 分块读取，避免超大 Vec 导致 IPC 失败
                    let mut buffer = vec![0u8; length as usize];
                    file.read_exact(&mut buffer)?;
                    
                    return Ok(response_builder
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, file_size))
                        .header(CONTENT_LENGTH, length.to_string())
                        .body(buffer)?);
                }
            }
        }
    }

    // 3. 无 Range，返回整个文件
    let mut buffer = vec![0u8; file_size as usize];
    file.read_exact(&mut buffer)?;
    
    Ok(response_builder
        .status(StatusCode::OK)
        .header(CONTENT_LENGTH, file_size)
        .body(buffer)?)
}
