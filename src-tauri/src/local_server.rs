use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
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
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let uri_str = parts[1];

    // 处理 CORS
    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 只处理 /proxy 路由（本地文件代理）
    if uri_str.starts_with("/proxy") {
        return handle_proxy_request(&mut stream, method, uri_str);
    }

    // 其他路由返回 404
    let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response.as_bytes())?;
    Ok(())
}

/// 处理本地文件代理请求
fn handle_proxy_request(
    stream: &mut TcpStream,
    method: &str,
    uri_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 解析路径
    let file_path = if let Ok(url) = Url::parse(&format!("http://localhost{}", uri_str)) {
        url.query_pairs()
            .find(|(key, _)| key == "path")
            .map(|(_, val)| val.into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if file_path.is_empty() || !std::path::Path::new(&file_path).exists() {
        let response = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 获取文件信息
    let mut file = std::fs::File::open(&file_path)?;
    let file_size = file.metadata()?.len();

    // MIME
    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime_type = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };

    // 解析 Range
    let mut range_start = 0u64;
    let mut range_end = file_size - 1;
    let mut is_partial = false;

    // 从请求头中解析 Range（简化处理，实际应该在 handle_connection 中解析）
    // 这里省略 Range 解析，直接返回整个文件

    // 处理 HEAD 请求
    if method == "HEAD" {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            mime_type, file_size
        );
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // 流式传输文件
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        mime_type, file_size
    );
    stream.write_all(response.as_bytes())?;

    // 8KB 缓冲区流式读取
    let mut remaining = file_size;
    let mut chunk = [0u8; 8192];
    while remaining > 0 {
        let to_read = remaining.min(chunk.len() as u64) as usize;
        let n = file.read(&mut chunk[..to_read])?;
        if n == 0 {
            break;
        }
        if stream.write_all(&chunk[..n]).is_err() {
            log::debug!("[LocalServer] Client closed connection");
            break;
        }
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

pub fn get_server_port() -> Option<u16> {
    LOCAL_SERVER.get().map(|s| *s.actual_port.lock().unwrap())
}
