use std::collections::HashMap;
use std::io::{Read, Write, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;

/// 本地文件服务器状态
pub struct LocalFileServer {
    port: u16,
    actual_port: Mutex<u16>,
    file_paths: Arc<Mutex<HashMap<String, String>>>, // path_id -> file_path
}

impl LocalFileServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            actual_port: Mutex::new(port),
            file_paths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 注册一个文件路径，返回访问 URL
    pub fn register_file(&self, file_path: String) -> String {
        let id = format!("{}", md5::compute(&file_path));
        let mut paths = self.file_paths.lock().unwrap();
        paths.insert(id.clone(), file_path);
        let port = *self.actual_port.lock().unwrap();
        format!("http://localhost:{}/?file={}", port, id)
    }

    /// 启动服务器，如果端口被占用则自动尝试其他端口
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
                    log::warn!("[LocalServer] Port {} in use, trying {}", port, port + 1);
                    port += 1;
                }
                Err(e) => return Err(e.into()),
            }
        };
        
        // 保存实际使用的端口
        *self.actual_port.lock().unwrap() = port;

        let file_paths = self.file_paths.clone();

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let paths = file_paths.clone();
                        thread::spawn(move || {
                            if let Err(e) = handle_connection(stream, paths) {
                                log::error!("[LocalServer] Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("[LocalServer] Accept error: {}", e);
                    }
                }
            }
        });

        Ok(port)
    }
}

fn handle_connection(
    mut stream: TcpStream,
    file_paths: Arc<Mutex<HashMap<String, String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0u8; 1024];
    let n = stream.read(&mut buffer)?;

    let request = String::from_utf8_lossy(&buffer[..n]);
    let lines: Vec<&str> = request.lines().collect();
    
    if lines.is_empty() {
        return Ok(());
    }

    // 解析请求行: GET /file_id HTTP/1.1
    let parts: Vec<&str> = lines[0].split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let path = parts[1];
    
    // 从查询参数 ?file= 中提取 file_id
    let file_id = if let Some(query_start) = path.find('?') {
        let query = &path[query_start + 1..];
        let mut id = "";
        for param in query.split('&') {
            if let Some((key, value)) = param.split_once('=') {
                if key == "file" {
                    id = value;
                    break;
                }
            }
        }
        id
    } else {
        path.trim_start_matches('/')
    };

    // 解析 Range 头
    let mut range_start: u64 = 0;
    let mut range_end: Option<u64> = None;
    
    for line in &lines {
        if line.to_lowercase().starts_with("range:") {
            if let Some(range_val) = line.split(':').nth(1) {
                let range_val = range_val.trim();
                if let Some(bytes_val) = range_val.strip_prefix("bytes=") {
                    let range_parts: Vec<&str> = bytes_val.split('-').collect();
                    if range_parts.len() == 2 {
                        range_start = range_parts[0].parse().unwrap_or(0);
                        range_end = range_parts[1].parse().ok();
                    }
                }
            }
            break;
        }
    }

    log::info!("[server] 请求: {}", path);
    log::info!("[server] file_id: {}", file_id);
    
    // 查找文件 - 先尝试按 MD5 ID 查找，找不到时直接使用 file_id 作为路径
    let file_path = {
        let paths = file_paths.lock().unwrap();
        if let Some(p) = paths.get(file_id) {
            p.clone()
        } else {
            // 未找到 ID，尝试直接使用 file_id 作为文件路径
            let direct_path = urlencoding::decode(file_id).unwrap_or_else(|_| file_id.into()).to_string();
            if std::path::Path::new(&direct_path).exists() {
                direct_path
            } else {
                let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                stream.write_all(response.as_bytes())?;
                return Ok(());
            }
        }
    };

    // 读取文件
    let mut file = match std::fs::File::open(&file_path) {
        Ok(f) => f,
        Err(_) => {
            let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }
    };

    let file_size = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => {
            let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }
    };

    // 获取 MIME 类型
    let ext = std::path::Path::new(&file_path)
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
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };

    // 处理 Range
    let (start, end, is_partial) = if range_end.is_some() || range_start > 0 {
        let end = range_end.unwrap_or(file_size - 1).min(file_size - 1);
        (range_start, end, true)
    } else {
        (0, file_size - 1, false)
    };

    let content_length = end - start + 1;

    // Seek 并读取
    if let Err(_) = file.seek(SeekFrom::Start(start)) {
        let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    let mut buffer = vec![0u8; content_length as usize];
    if let Err(_) = file.read_exact(&mut buffer) {
        let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 构建响应
    let status = if is_partial { "206 Partial Content" } else { "200 OK" };
    let range_header = if is_partial {
        format!("Content-Range: bytes {}-{}/{}\r\n", start, end, file_size)
    } else {
        String::new()
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\n{}\r\n",
        status, mime_type, content_length, range_header
    );

    stream.write_all(response.as_bytes())?;
    stream.write_all(&buffer)?;
    Ok(())
}

// 简单的 md5 实现（避免额外依赖）
mod md5 {
    pub fn compute(input: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// 全局服务器实例
static LOCAL_SERVER: once_cell::sync::OnceCell<LocalFileServer> = once_cell::sync::OnceCell::new();

/// 初始化本地服务器，返回实际使用的端口号
pub fn init_local_server(port: u16) -> Result<u16, Box<dyn std::error::Error>> {
    let server = LocalFileServer::new(port);
    let actual_port = server.start()?;
    LOCAL_SERVER.set(server).ok();
    Ok(actual_port)
}

/// 获取文件 URL
pub fn get_file_url(file_path: String) -> Option<String> {
    if let Some(server) = LOCAL_SERVER.get() {
        Some(server.register_file(file_path))
    } else {
        None
    }
}
