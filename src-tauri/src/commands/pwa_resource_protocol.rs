use tauri::http::{Request, Response};
use std::path::{PathBuf, Path};
use std::fs;
use tauri::Manager;
use url::Url;

// 缓存白名单
const CACHE_WHITELIST: &[&str] = &[
    "html", "htm", "js", "mjs", "cjs", "css", 
    "json", "webmanifest",
    "woff", "woff2", "ttf", "otf", "svg", "map"
];

pub fn handle_resource_request(
    app: &tauri::AppHandle,
    request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error>> {
    let uri = request.uri().to_string();
    let headers = request.headers();
    let url_obj = Url::parse(&uri)?;
    let uri_path = url_obj.path();
    log::info!("[PWAResource] Raw URI: {}, path: {}", uri, uri_path);
    
    // 0. 检测本地开发地址，直接代理而不走缓存逻辑
    let is_local_dev = uri_path.contains("localhost") 
        || uri_path.contains("127.0.0.1")
        || uri_path.contains("192.168.")
        || uri_path.contains("10.0.");
    
    if is_local_dev {
        let original_url = if let Some(pos) = uri.find("/http/") {
            let path_part = &uri[pos + 6..];
            let real_path = path_part.replace(".port-", ":");
            format!("http://{}", real_path)
        } else if let Some(pos) = uri.find("/https/") {
            let path_part = &uri[pos + 7..];
            let real_path = path_part.replace(".port-", ":");
            format!("https://{}", real_path)
        } else {
            uri.clone()
        };
        
        log::info!("[PWAResource] Local dev mode, direct proxy: {}", original_url);
        
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        let resp = match client.get(&original_url).send() {
            Ok(r) => r,
            Err(e) => {
                return Ok(Response::builder()
                    .status(502)
                    .body(format!("Proxy Error: {}", e).into_bytes())?);
            }
        };
        
        let status = resp.status().as_u16();
        let mime = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        
        let content = resp.bytes()?.to_vec();
        
        let mut response_builder = Response::builder()
            .status(status)
            .header("Access-Control-Allow-Origin", "*");
        
        if let Some(m) = mime {
            response_builder = response_builder.header("Content-Type", m);
        }
        
        return Ok(response_builder.body(content)?);
    }
    
    // 1. 解析请求
    // 情况A: /https/y-ymeow.github.io/musicplayer-pwa/index.html
    // 情况B: /assets/xxx.js (相对路径请求，需要从Referer恢复)
    
    let (proto, domain, file_path, ctx_str) = 
        if let Some(pos) = uri_path.find("/https/") {
            // 直接请求，提取完整路径
            let after = &uri_path[pos + 7..]; // y-ymeow.github.io/musicplayer-pwa/index.html
            // show uri path in log
            log::info!("[PWAResource] URI Path: {}", after);
            parse_path("https", after, uri_path)
        } else if let Some(pos) = uri_path.find("/http/") {
            let after = &uri_path[pos + 6..];
            parse_path("http", after, uri_path)
        } else {
            // 相对路径请求如 /assets/xxx.js，从 Referer 恢复
            resolve_relative_path(uri_path, headers, &uri)?
        };
    
    log::info!("[PWAResource] Parsed: proto={}, domain={}, path='{}'", proto, domain, file_path);

    // 2. 构建原始 URL
    let query = url_obj.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let original_url = format!("{}://{}{}{}", proto, domain, file_path, query);
    log::info!("[PWAResource] Proxy: {} -> {}", uri, original_url);

    // 3. 缓存处理
    let domain_name: &str = domain.split(':').next().unwrap_or("unknown");
    let ext = Path::new(&file_path).extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let should_cache = CACHE_WHITELIST.contains(&ext.as_str());

    let app_data_dir = app.path().app_data_dir()?;
    let cache_dir = app_data_dir.join("pwa_cache").join(&domain_name);
    let local_file_path = if file_path == "/" || file_path.is_empty() {
        cache_dir.join("index.html")
    } else {
        cache_dir.join(file_path.trim_start_matches('/'))
    };

    // 4. 加载资源
    let (content, remote_mime, status) = 
        match fetch_from_network(&original_url, should_cache, &local_file_path, proto, &domain, &file_path) {
            Ok((c, m, s)) if s == 200 => (c, m, s),
            Ok((c, m, s)) => {
                if should_cache && local_file_path.exists() {
                    log::warn!("[PWAResource] Network returned {}, trying cache", s);
                    match fs::read(&local_file_path) {
                        Ok(cache_content) => (cache_content, None, 200),
                        Err(_) => (c, m, s)
                    }
                } else {
                    (c, m, s)
                }
            }
            Err(e) => {
                if should_cache && local_file_path.exists() {
                    log::warn!("[PWAResource] Network error: {}, using cache", e);
                    match fs::read(&local_file_path) {
                        Ok(cache_content) => (cache_content, None, 200),
                        Err(_) => return Err(e)
                    }
                } else {
                    return Err(e);
                }
            }
        };

    // 5. 响应
    let mut response = build_response(&local_file_path, content, remote_mime, status)?;
    response.headers_mut().insert(
        "Set-Cookie",
        format!("pwa_context={}; Path=/; Max-Age=3600; SameSite=Lax", ctx_str).parse().unwrap()
    );
    Ok(response)
}

// 解析直接请求的路径
fn parse_path<'a>(proto: &'a str, after_proto: &str, _uri_path: &str) -> (&'a str, String, String, String) {
    // after_proto: y-ymeow.github.io/musicplayer-pwa/index.html
    let parts: Vec<&str> = after_proto.splitn(2, '/').collect();
    // show parts in log
    log::info!("[PWAResource] After Proto: {}", parts[0]);
    log::info!("[PWAResource] After Proto: {}", parts[1]);
    let domain = parts[0].to_string();
    
    let file_path = if parts.len() > 1 {
        format!("/{}", parts[1])
    } else {
        "/".to_string()
    };

    log::info!("[PWAResource] Parsed: proto={}, domain={}, path='{}'", proto, domain, file_path);
    
    let ctx_str = format!("{}/{}", proto, domain);
    (proto, domain, file_path, ctx_str)
}

// 解析相对路径请求
fn resolve_relative_path(
    uri_path: &str,
    headers: &tauri::http::HeaderMap,
    _uri: &str
) -> Result<(&'static str, String, String, String), Box<dyn std::error::Error>> {
    // 从 Referer 获取基础路径
    // Referer: pwa-resource://localhost/https/y-ymeow.github.io/musicplayer-pwa/index.html
    // show headers in log
    println!("[PWAResource] Headers: {:?}", headers);
    let referer = headers.get("referer")
        .and_then(|r| r.to_str().ok())
        .ok_or("Missing referer for relative path")?;
    
    log::info!("[PWAResource] Resolving relative path '{}' from referer: {}", uri_path, referer);
    
    // 提取 referer 中的协议、域名和基础路径
    let (proto, after_proto) = if let Some(pos) = referer.find("/https/") {
        ("https", &referer[pos + 7..])
    } else if let Some(pos) = referer.find("/http/") {
        ("http", &referer[pos + 6..])
    } else {
        return Err("Invalid referer format".into());
    };
    
    // after_proto: y-ymeow.github.io/musicplayer-pwa/index.html
    let parts: Vec<&str> = after_proto.splitn(2, '/').collect();
    let domain = parts[0].to_string();
    
    // 提取基础路径（去掉文件名）
    let base_path = if parts.len() > 1 {
        let path_part = parts[1];
        // 找到最后一个 / 之前的部分
        if let Some(last_slash) = path_part.rfind('/') {
            format!("/{}/", &path_part[..last_slash])
        } else {
            "/".to_string()
        }
    } else {
        "/".to_string()
    };
    
    // 组合完整路径: base_path + uri_path
    // base_path: /musicplayer-pwa/
    // uri_path: /assets/xxx.js
    // result: /musicplayer-pwa/assets/xxx.js
    let file_path = if uri_path.starts_with('/') {
        format!("{}{}", base_path.trim_end_matches('/'), uri_path)
    } else {
        format!("{}/{}", base_path.trim_end_matches('/'), uri_path)
    };
    
    let ctx_str = format!("{}/{}", proto, domain);
    log::info!("[PWAResource] Resolved: base='{}', file='{}'", base_path, file_path);
    
    Ok((proto, domain, file_path, ctx_str))
}

fn fetch_from_network(
    url: &str, 
    should_cache: bool, 
    cache_path: &PathBuf,
    proto: &str,
    domain: &str,
    file_path: &str
) -> Result<(Vec<u8>, Option<String>, u16), Box<dyn std::error::Error>> {
    log::info!("[PWAResource] Fetching: {}", url);
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    
    let mut resp = match client.get(url).send() {
        Ok(r) => r,
        Err(e) => return Ok((format!("Error: {}", e).into_bytes(), None, 502)),
    };

    let status = resp.status().as_u16();
    let mime = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let mut content = Vec::new();
    let _ = resp.copy_to(&mut content);

    // 如果是 HTML，替换所有相对路径为绝对路径
    if status == 200 && mime.as_ref().map(|m| m.contains("html")).unwrap_or(false) {
        if let Ok(mut html) = String::from_utf8(content.clone()) {
            // 计算基础路径: pwa-resource://localhost/https/domain/path/
            let base_file_path = if let Some(last_slash) = file_path.rfind('/') {
                &file_path[..last_slash + 1]
            } else {
                "/"
            };
            let base_url = format!("pwa-resource://localhost/{}/{}{}", proto, domain, base_file_path);
            
            // 替换各种相对路径引用
            // 1. src="./xxx" -> src="pwa-resource://.../xxx"
            // 2. href="./xxx" -> href="pwa-resource://.../xxx"
            // 3. url(./xxx) -> url(pwa-resource://.../xxx)
            // 4. src="/xxx" -> src="pwa-resource://.../xxx" (相对于根)
            
            // 使用正则替换各种属性中的路径
            html = html.replace("src=\"./", &format!("src=\"{}", base_url));
            html = html.replace("src='./", &format!("src='{}", base_url));
            html = html.replace("href=\"./", &format!("href=\"{}", base_url));
            html = html.replace("href='./", &format!("href='{}", base_url));
            html = html.replace("src=\"/", &format!("src=\"{}", base_url));
            html = html.replace("src='/", &format!("src='{}", base_url));
            html = html.replace("href=\"/", &format!("href=\"{}", base_url));
            html = html.replace("href='/", &format!("href='{}", base_url));
            
            // 处理 CSS url()
            html = html.replace("url(./", &format!("url({}", base_url));
            html = html.replace("url(/", &format!("url({}", base_url));
            
            // 处理 import "./xxx"
            html = html.replace("import \"./", &format!("import \"{}", base_url));
            html = html.replace("import './", &format!("import '{}", base_url));
            
            log::info!("[PWAResource] Replaced relative paths with base: {}", base_url);
            
            log::info!("[PWAResource] Replaced relative paths with base: {}", base_url);
            content = html.into_bytes();
        }
    }

    if status == 200 && should_cache {
        let _ = fs::create_dir_all(cache_path.parent().unwrap());
        let _ = fs::write(cache_path, &content);
    }
    Ok((content, mime, status))
}

fn build_response(path: &PathBuf, content: Vec<u8>, mime_override: Option<String>, status: u16) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error>> {
    let mime = if let Some(m) = mime_override { m } else {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "html" | "htm" => "text/html".to_string(),
            "js" | "mjs" | "cjs" | "ts" | "mts" | "jsx" | "tsx" => "application/javascript".to_string(),
            "css" => "text/css".to_string(),
            "json" | "map" => "application/json".to_string(),
            "png" => "image/png".to_string(),
            "jpg" | "jpeg" => "image/jpeg".to_string(),
            "svg" => "image/svg+xml".to_string(),
            "webp" => "image/webp".to_string(),
            "woff" | "woff2" | "ttf" | "otf" => format!("font/{}", ext),
            _ => "application/octet-stream".to_string(),
        }
    };

    Ok(Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .body(content)?)
}
