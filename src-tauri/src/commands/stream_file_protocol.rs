use http::{header::*, Response, StatusCode};
use std::io::{Read, Seek, SeekFrom};

/// 处理流式文件请求，支持 Range
pub fn handle_stream_request(
    request: http::Request<Vec<u8>>,
) -> Result<http::Response<Vec<u8>>, Box<dyn std::error::Error>> {
    let path = request.uri().path();

    // URL 解码
    let decoded_path = match urlencoding::decode(path) {
        Ok(p) => p.to_string(),
        Err(_) => path.to_string(),
    };

     log::info!("[stream headers] {:?}", request.headers());

    // 去掉开头的 /
    let file_path = if decoded_path.starts_with('/') {
        decoded_path[1..].to_string()
    } else {
        decoded_path
    };

    log::info!("[stream] 请求: {}", file_path);

    // 打开文件
    let mut file = std::fs::File::open(&file_path)?;

    // 获取文件大小
    let file_size = file.metadata()?.len();

    // 获取 MIME 类型
    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    log::info!("[stream] 文件大小: {}", file_size);
    log::info!("[stream] 文件类型: {}", ext);

    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };

    let mut response_builder = Response::builder()
        .header(CONTENT_TYPE, mime_type)
        .header(ACCEPT_RANGES, "bytes")
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*");


    // 检查 Range 请求头
    // 严格按照请求的 Range 返回，不扩展数据量
    let range_header = request.headers().get(RANGE).and_then(|v| v.to_str().ok());
    
    if let Some(range_str) = range_header {
        log::info!("[stream] Range: {}", range_str);

        if let Some(range_val) = range_str.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_val.split('-').collect();
            if parts.len() == 2 {
                let start: u64 = parts[0].parse().unwrap_or(0);
                let end: u64 = if parts[1].is_empty() {
                    // end 为空，返回从 start 到文件末尾
                    file_size - 1
                } else {
                    parts[1].parse().unwrap_or(file_size - 1)
                };
                
                let end = end.min(file_size - 1);

                if start <= end && start < file_size {
                    let length = end - start + 1;
                    
                    file.seek(SeekFrom::Start(start))?;
                    let mut buffer = vec![0u8; length as usize];
                    file.read_exact(&mut buffer)?;
                    
                    log::info!("[stream] 206 Partial Content: bytes {}-{}/{} ({} bytes)", start, end, file_size, length);
                    return Ok(response_builder
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, file_size))
                        .header(CONTENT_LENGTH, length.to_string())
                        .header(CONTENT_ENCODING, "identity")
                        .body(buffer)?);
                }
            }
        }
    }

    // 无 Range，返回整个文件
    // 8MB 的 MP3 直接读取，现代设备完全可以处理
    log::info!("[stream] 返回整个文件: {} bytes", file_size);
    
    let mut buffer = vec![0u8; file_size as usize];
    file.read_exact(&mut buffer)?;
    
    Ok(response_builder
        .header(CONTENT_LENGTH, file_size)
        .body(buffer)?)
}
