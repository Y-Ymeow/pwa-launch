use std::io::{Read, Write, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::sync::Mutex;
use std::thread;
use url::Url;

/// 本地文件服务器状态
pub struct LocalFileServer {
    port: u16,
    actual_port: Mutex<u16>,
}

impl LocalFileServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            actual_port: Mutex::new(port),
        }
    }

    /// 生成直接访问本地文件的 URL (无状态模式，解决只能存一个文件的问题)
    pub fn get_proxy_url(&self, file_path: String) -> String {
        let port = *self.actual_port.lock().unwrap();
        let encoded_path = urlencoding::encode(&file_path);
        format!("http://localhost:{}/proxy?path={}", port, encoded_path)
    }

    /// 生成 PWA 资源 URL
    /// original_url: https://y-ymeow.github.io/musicplayer-pwa/index.html
    /// result: http://localhost:PORT/pwa/https/y-ymeow.github.io/musicplayer-pwa/index.html
    pub fn get_pwa_url(&self, original_url: &str) -> String {
        let port = *self.actual_port.lock().unwrap();
        
        // 去掉协议前缀
        let path = if original_url.starts_with("https://") {
            format!("https/{}", &original_url[8..])
        } else if original_url.starts_with("http://") {
            format!("http/{}", &original_url[7..])
        } else {
            return original_url.to_string();
        };
        
        format!("http://localhost:{}/pwa/{}", port, path)
    }

    /// 启动服务器
    pub fn start(&self) -> Result<u16, Box<dyn std::error::Error>> {
        let mut port = self.port;
        let listener = loop {
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
            match TcpListener::bind(addr) {
                Ok(l) => {
                    log::info!("[LocalServer] Listening on http://{}", addr);
                    break l;
                }
                Err(_) if port < 65535 => {
                    port += 1;
                }
                Err(e) => return Err(e.into()),
            }
        };
        
        *self.actual_port.lock().unwrap() = port;

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        thread::spawn(move || {
                            if let Err(e) = handle_connection(stream) {
                                log::debug!("[LocalServer] Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => log::error!("[LocalServer] Accept error: {}", e),
                }
            }
        });

        Ok(port)
    }
}

fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0u8; 4096];
    let n = stream.read(&mut buffer)?;
    if n == 0 { return Ok(()); }

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 { return Ok(()); }

    let method = parts[0];
    let uri_str = parts[1];
    
    // 解析 headers
    let mut headers = tauri::http::HeaderMap::new();
    for line in request.lines().skip(1) {
        if line.is_empty() { break; }
        if let Some((key, value)) = line.split_once(": ") {
            if let Ok(header_name) = key.parse::<tauri::http::HeaderName>() {
                if let Ok(header_value) = value.parse::<tauri::http::HeaderValue>() {
                    headers.insert(header_name, header_value);
                }
            }
        }
    }

    // 处理 CORS
    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 处理 PWA 资源路由 /pwa/https/domain/path
    if uri_str.starts_with("/pwa/") {
        return handle_pwa_request(&mut stream, method, uri_str, &headers);
    }
    
    // 处理绝对路径请求（如 /manga-reader-pwa/assets/xxx.js）
    // 从 Referer 恢复上下文
    if uri_str.starts_with("/") && !uri_str.starts_with("/proxy") {
        return handle_absolute_path_request(&mut stream, method, uri_str, &headers);
    }

    // 解析路径 (原有逻辑)
    let file_path = if let Ok(url) = Url::parse(&format!("http://localhost{}", uri_str)) {
        url.query_pairs()
            .find(|(key, _)| key == "path")
            .map(|(_, val)| val.into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if file_path.is_empty() || !std::path::Path::new(&file_path).exists() {
        let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 获取文件信息
    let mut file = std::fs::File::open(&file_path)?;
    let file_size = file.metadata()?.len();
    
    // MIME
    let ext = std::path::Path::new(&file_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "application/octet-stream",
    };

    // 解析 Range
    let mut range_start = 0u64;
    let mut range_end = file_size - 1;
    let mut is_partial = false;

    for line in request.lines() {
        if line.to_lowercase().starts_with("range: bytes=") {
            let range_val = &line[13..];
            let parts: Vec<&str> = range_val.split('-').collect();
            if parts.len() == 2 {
                range_start = parts[0].parse().unwrap_or(0);
                if !parts[1].is_empty() {
                    range_end = parts[1].parse().unwrap_or(file_size - 1);
                }
                range_end = range_end.min(file_size - 1);
                is_partial = true;
            }
            break;
        }
    }

    // 处理 HEAD 请求
    if method == "HEAD" {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
            mime_type, file_size
        );
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 构建响应头
    let status = if is_partial { "206 Partial Content" } else { "200 OK" };
    let content_length = range_end - range_start + 1;
    let range_header = if is_partial {
        format!("Content-Range: bytes {}-{}/{}\r\n", range_start, range_end, file_size)
    } else {
        String::new()
    };

    let response_headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\n{}\r\n",
        status, mime_type, content_length, range_header
    );

    stream.write_all(response_headers.as_bytes())?;

    // 发送数据
    file.seek(SeekFrom::Start(range_start))?;
    let mut remaining = content_length;
    let mut chunk = [0u8; 65536];
    while remaining > 0 {
        let to_read = remaining.min(chunk.len() as u64) as usize;
        let n = file.read(&mut chunk[..to_read])?;
        if n == 0 { break; }
        if let Err(_) = stream.write_all(&chunk[..n]) { break; }
        remaining -= n as u64;
    }

    Ok(())
}

static LOCAL_SERVER: once_cell::sync::OnceCell<LocalFileServer> = once_cell::sync::OnceCell::new();

pub fn init_local_server(port: u16) -> Result<u16, Box<dyn std::error::Error>> {
    let server = LocalFileServer::new(port);
    let actual_port = server.start()?;
    LOCAL_SERVER.set(server).ok();
    Ok(actual_port)
}

pub fn get_file_url(file_path: String) -> Option<String> {
    LOCAL_SERVER.get().map(|s| s.get_proxy_url(file_path))
}

/// 获取 PWA 资源的本地 HTTP URL
/// 将 https://y-ymeow.github.io/musicplayer-pwa/index.html
/// 转换为 http://localhost:PORT/pwa/https/y-ymeow.github.io/musicplayer-pwa/index.html
pub fn get_pwa_url(original_url: &str) -> Option<String> {
    LOCAL_SERVER.get().map(|s| s.get_pwa_url(original_url))
}

/// 获取本地服务器端口号
pub fn get_server_port() -> Option<u16> {
    LOCAL_SERVER.get().map(|s| *s.actual_port.lock().unwrap())
}

/// 处理 PWA 资源请求
/// URL 格式: /pwa/https/y-ymeow.github.io/musicplayer-pwa/index.html
fn handle_pwa_request(stream: &mut TcpStream, method: &str, uri_str: &str, _headers: &tauri::http::HeaderMap) -> Result<(), Box<dyn std::error::Error>> {
    // 解析 URL: /pwa/https/domain/path
    let path = &uri_str[5..]; // 去掉 /pwa/ 前缀 → https/y-ymeow.github.io/musicplayer-pwa/index.html
    
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() < 1 {
        return send_404(stream);
    }
    
    let proto = parts[0]; // https 或 http
    let rest = if parts.len() > 1 { parts[1] } else { "" }; // y-ymeow.github.io/musicplayer-pwa/index.html
    
    // 再次分割域名和路径
    let domain_path: Vec<&str> = rest.splitn(2, '/').collect();
    let domain = domain_path[0];
    let file_path = if domain_path.len() > 1 {
        format!("/{}", domain_path[1])
    } else {
        "/".to_string()
    };
    
    // 构建原始 URL
    let original_url = format!("{}://{}{}", proto, domain, file_path);
    log::info!("[LocalServer] PWA request: {} -> {}", uri_str, original_url);
    
    // 流式发送资源
    send_pwa_resource(stream, &original_url, method)
}

/// 发送 PWA 资源（流式传输，优先缓存）
fn send_pwa_resource(
    stream: &mut TcpStream,
    url: &str,
    method: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    use std::io::{BufReader, BufWriter, Write, copy};
    
    // 检查是否是本地开发地址
    let is_local_dev = url.contains("localhost") 
        || url.contains("127.0.0.1")
        || url.contains("192.168.")
        || url.contains("10.0.");
    
    if is_local_dev {
        // 直接代理本地请求（流式）
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        let resp = client.get(url).send()?;
        let status = resp.status().as_u16();
        let mime = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        
        // 获取内容长度（如果有）
        let content_length = resp.content_length();
        
        // 发送响应头
        let response = if let Some(len) = content_length {
            format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
                if status == 200 { "200 OK" } else { "404 Not Found" },
                mime, len
            )
        } else {
            format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nTransfer-Encoding: chunked\r\n\r\n",
                if status == 200 { "200 OK" } else { "404 Not Found" },
                mime
            )
        };
        stream.write_all(response.as_bytes())?;
        
        if method != "HEAD" && status == 200 {
            // 流式发送 body
            use std::io::Read;
            let mut body_reader = resp;
            let mut buf = [0u8; 8192];
            
            if content_length.is_some() {
                // 有 Content-Length，直接流式传输
                loop {
                    match body_reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = stream.write_all(&buf[..n]) {
                                log::warn!("[LocalServer] Client closed connection: {}", e);
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            log::warn!("[LocalServer] Read error: {}", e);
                            return Ok(());
                        }
                    }
                }
            } else {
                // 没有 Content-Length，使用 chunked encoding
                loop {
                    match body_reader.read(&mut buf) {
                        Ok(0) => {
                            let _ = stream.write_all(b"0\r\n\r\n");
                            break;
                        }
                        Ok(n) => {
                            if stream.write_all(format!("{:X}\r\n", n).as_bytes()).is_err()
                                || stream.write_all(&buf[..n]).is_err()
                                || stream.write_all(b"\r\n").is_err() {
                                log::warn!("[LocalServer] Client closed connection during chunked transfer");
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            log::warn!("[LocalServer] Read error: {}", e);
                            return Ok(());
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    
    // 解析 URL 获取缓存路径
    let parsed = Url::parse(url)?;
    let domain = parsed.host_str().unwrap_or("unknown");
    let path = parsed.path();
    
    let cache_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.pwa-container.app")
        .join("pwa_cache")
        .join(domain);
    
    let cache_file = if path == "/" || path.is_empty() || path.ends_with('/') {
        // 目录路径，使用 index.html
        let dir_path = if path == "/" || path.is_empty() {
            cache_dir.clone()
        } else {
            cache_dir.join(path.trim_start_matches('/').trim_end_matches('/'))
        };
        dir_path.join("index.html")
    } else {
        cache_dir.join(path.trim_start_matches('/'))
    };
    
    // 检查缓存是否存在（优先使用缓存，避免网络延迟）
    if cache_file.exists() {
        let metadata = std::fs::metadata(&cache_file)?;
        let file_size = metadata.len();
        
        // 检测 MIME 类型
        // 1. 如果是目录路径（以 / 结尾），默认为 index.html
        // 2. 根据文件扩展名判断
        let mime = if path.ends_with('/') || path.is_empty() {
            "text/html"
        } else {
            let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            match ext.as_str() {
                "html" | "htm" => "text/html",
                "js" | "mjs" | "cjs" => "application/javascript",
                "css" => "text/css",
                "json" => "application/json",
                "woff" => "font/woff",
                "woff2" => "font/woff2",
                "ttf" => "font/ttf",
                "otf" => "font/otf",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            }
        };
        
        log::info!("[LocalServer] Serving from cache: {} ({} bytes)", cache_file.display(), file_size);
        
        // 使用 chunked encoding，避免 Content-Length 不匹配问题
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nTransfer-Encoding: chunked\r\n\r\n",
            mime
        );
        if let Err(e) = stream.write_all(response.as_bytes()) {
            log::warn!("[LocalServer] Failed to send headers: {}", e);
            return Ok(());
        }
        
        if method != "HEAD" {
            // 流式读取并发送文件（chunked encoding）
            let file = match std::fs::File::open(&cache_file) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("[LocalServer] Failed to open cache file: {}", e);
                    let _ = stream.write_all(b"0\r\n\r\n");
                    return Ok(());
                }
            };
            
            let mut reader = BufReader::with_capacity(64 * 1024, file);
            let mut buf = [0u8; 8192];
            
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // 发送结束标记
                        if stream.write_all(b"0\r\n\r\n").is_err() {
                            log::debug!("[LocalServer] Client closed before end marker");
                        }
                        break;
                    }
                    Ok(n) => {
                        // 发送 chunk size 和数据
                        if stream.write_all(format!("{:X}\r\n", n).as_bytes()).is_err()
                            || stream.write_all(&buf[..n]).is_err()
                            || stream.write_all(b"\r\n").is_err() {
                            log::debug!("[LocalServer] Client closed during file transfer");
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        log::error!("[LocalServer] Read error from cache: {}", e);
                        let _ = stream.write_all(b"0\r\n\r\n");
                        break;
                    }
                }
            }
        }
        
        // 异步更新缓存（后台线程，不阻塞当前响应）
        let url = url.to_string();
        let cache_file = cache_file.clone();
        std::thread::spawn(move || {
            if let Err(e) = update_cache_in_background(&url, &cache_file) {
                log::debug!("[LocalServer] Background cache update failed: {}", e);
            }
        });
        
        return Ok(());
    }
    
    // 缓存不存在，从网络获取
    log::info!("[LocalServer] Cache miss, fetching from network: {}", url);
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    
    match client.get(url).send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mime = resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            
            // 检测是否应该缓存
            // 1. 目录路径（以 / 结尾）视为 index.html，需要缓存
            // 2. 根据扩展名判断
            let should_cache = if path.ends_with('/') || path.is_empty() {
                true // 目录默认缓存（index.html）
            } else {
                let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                matches!(ext.as_str(), 
                    "html" | "htm" | "js" | "mjs" | "cjs" | "css" | "json" | "webmanifest" |
                    "woff" | "woff2" | "ttf" | "otf" | "svg" | "map"
                )
            };
            
            if status == 200 && should_cache {
                // 先读取整个内容到内存
                let content = resp.bytes()?.to_vec();
                
                // 写入缓存
                if let Some(parent) = cache_file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&cache_file, &content);
                
                // 使用 Content-Length 发送（更可靠）
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
                    mime,
                    content.len()
                );
                if let Err(e) = stream.write_all(response.as_bytes()) {
                    log::warn!("[LocalServer] Failed to send headers: {}", e);
                    return Ok(());
                }
                
                if method != "HEAD" {
                    if let Err(e) = stream.write_all(&content) {
                        log::warn!("[LocalServer] Failed to send body: {}", e);
                        return Ok(());
                    }
                }
            } else {
                // 不需要缓存，直接流式转发
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nTransfer-Encoding: chunked\r\n\r\n",
                    if status == 200 { "200 OK" } else { "404 Not Found" },
                    mime
                );
                if let Err(e) = stream.write_all(response.as_bytes()) {
                    log::warn!("[LocalServer] Failed to send headers: {}", e);
                    return Ok(());
                }
                
                if method != "HEAD" && status == 200 {
                    use std::io::Read;
                    let mut body_reader = resp;
                    let mut buf = [0u8; 8192];
                    loop {
                        match body_reader.read(&mut buf) {
                            Ok(0) => {
                                let _ = stream.write_all(b"0\r\n\r\n");
                                break;
                            }
                            Ok(n) => {
                                if stream.write_all(format!("{:X}\r\n", n).as_bytes()).is_err()
                                    || stream.write_all(&buf[..n]).is_err()
                                    || stream.write_all(b"\r\n").is_err() {
                                    log::warn!("[LocalServer] Client closed connection during forward");
                                    return Ok(());
                                }
                            }
                            Err(e) => {
                                log::warn!("[LocalServer] Read error during forward: {}", e);
                                return Ok(());
                            }
                        }
                    }
                }
            }
            
            Ok(())
        }
        Err(e) => {
            let response = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n{}",
                e.to_string().len(),
                e
            );
            stream.write_all(response.as_bytes())?;
            Ok(())
        }
    }
}

/// 后台更新缓存（不阻塞主响应）
fn update_cache_in_background(url: &str, cache_file: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    
    let resp = client.get(url).send()?;
    
    if resp.status().is_success() {
        let content = resp.bytes()?;
        let mut file = std::fs::File::create(cache_file)?;
        file.write_all(&content)?;
        log::info!("[LocalServer] Background cache updated: {}", cache_file.display());
    }
    
    Ok(())
}

/// 处理绝对路径请求（从 Referer 恢复上下文）
/// 例如: /manga-reader-pwa/assets/xxx.js
/// Referer: http://localhost:8765/pwa/https/y-ymeow.github.io/musicplayer-pwa/index.html
fn handle_absolute_path_request(
    stream: &mut TcpStream,
    method: &str,
    uri_str: &str,
    headers: &tauri::http::HeaderMap,
) -> Result<(), Box<dyn std::error::Error>> {
    // 从 Referer 提取上下文
    let referer = headers
        .get("referer")
        .and_then(|r| r.to_str().ok())
        .ok_or("Missing referer")?;

    log::info!(
        "[LocalServer] Absolute path request: {} from referer: {}",
        uri_str,
        referer
    );

    // 从 Referer 提取协议和域名
    // Referer: http://localhost:8765/pwa/https/y-ymeow.github.io/musicplayer-pwa/index.html
    let ctx_start = referer.find("/pwa/").map(|p| p + 5); // 跳过 /pwa/
    if ctx_start.is_none() {
        return send_404(stream);
    }

    let ctx = &referer[ctx_start.unwrap()..]; // https/y-ymeow.github.io/musicplayer-pwa/index.html
    let ctx_parts: Vec<&str> = ctx.splitn(2, '/').collect();
    if ctx_parts.len() < 2 {
        return send_404(stream);
    }

    let proto = ctx_parts[0]; // https
    let after_proto = ctx_parts[1]; // y-ymeow.github.io/musicplayer-pwa/index.html

    // 提取域名
    let domain_path: Vec<&str> = after_proto.splitn(2, '/').collect();
    let domain = domain_path[0];
    
    // 对于绝对路径请求（如 /manga-reader-pwa/assets/xxx.js）
    // 直接使用请求路径，因为它已经包含了完整路径（相对于域名根目录）
    // 只需要确保路径以 / 开头
    let full_path = if uri_str.starts_with('/') {
        uri_str.to_string()
    } else {
        format!("/{}", uri_str)
    };

    // 构建原始 URL
    let original_url = format!("{}://{}{}", proto, domain, full_path);
    log::info!(
        "[LocalServer] Resolved absolute path: {} -> {}",
        uri_str,
        original_url
    );

    // 流式发送资源
    send_pwa_resource(stream, &original_url, method)
}

fn send_404(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\n\r\n";
    stream.write_all(response.as_bytes())?;
    Ok(())
}
