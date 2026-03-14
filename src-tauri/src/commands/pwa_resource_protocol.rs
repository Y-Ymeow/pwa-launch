use tauri::http::{Request, Response};
use std::path::{PathBuf, Path};
use std::fs;
use std::sync::Mutex;
use std::collections::HashMap;
use tauri::Manager;
use url::Url;
use once_cell::sync::Lazy;

// 缓存白名单
const CACHE_WHITELIST: &[&str] = &[
    "html", "htm", "js", "mjs", "cjs", "css", 
    "json", "webmanifest",
    "woff", "woff2", "ttf", "otf", "svg", "map"
];

// 全局路径映射表: 将路径的第一级目录映射到对应的域名上下文
// 例如: "manga-reader-pwa" -> "https/y-ymeow.github.io"
static PATH_CONTEXT_MAP: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn handle_resource_request(
    app: &tauri::AppHandle,
    request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error>> {
    let uri = request.uri().to_string();
    let headers = request.headers();
    let url_obj = Url::parse(&uri)?;
    let uri_path = url_obj.path();
    
    // 0. 检测本地开发地址，直接代理而不走缓存逻辑
    let is_local_dev = uri_path.contains("localhost") 
        || uri_path.contains("127.0.0.1")
        || uri_path.contains("192.168.")
        || uri_path.contains("10.0.");
    
    if is_local_dev {
        // 提取原始 URL
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
        
        // 直接代理，不走缓存
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
    
    // 1. 提取或恢复上下文 (protocol/domain)
    let context = if let Some(pos) = uri.find("/https/") {
        let after = &uri[pos + 7..];
        let parts: Vec<&str> = after.split('/').collect();
        let domain = parts[0];
        let ctx = format!("https/{}", domain);
        
        // 自动学习：如果路径中有超过一级的目录，记录该目录与域名的关系
        if parts.len() > 1 && !parts[1].is_empty() && !parts[1].contains('.') {
            let mut map = PATH_CONTEXT_MAP.lock().unwrap();
            map.insert(parts[1].to_string(), ctx.clone());
        }
        Some(ctx)
    } else if let Some(pos) = uri.find("/http/") {
        let after = &uri[pos + 6..];
        let parts: Vec<&str> = after.split('/').collect();
        let domain = parts[0];
        let ctx = format!("http/{}", domain);
        if parts.len() > 1 && !parts[1].is_empty() && !parts[1].contains('.') {
            let mut map = PATH_CONTEXT_MAP.lock().unwrap();
            map.insert(parts[1].to_string(), ctx.clone());
        }
        Some(ctx)
    } else {
        // 绝对路径请求，尝试回溯
        let first_seg = uri_path.split('/').filter(|s| !s.is_empty()).next().unwrap_or("");
        let mut ctx = PATH_CONTEXT_MAP.lock().unwrap().get(first_seg).cloned();
        
        // Cookie 兜底
        if ctx.is_none() {
            ctx = headers.get("cookie")
                .and_then(|c| c.to_str().ok())
                .and_then(|s| s.split(';').find(|p| p.trim().starts_with("pwa_context=")))
                .map(|p| p.trim()["pwa_context=".len()..].to_string());
        }
        
        // Referer 兜底
        if ctx.is_none() {
            ctx = headers.get("referer").and_then(|r| r.to_str().ok()).and_then(|r| {
                if let Some(pos) = r.find("/https/") {
                    Some(format!("https/{}", r[pos + 7..].split('/').next().unwrap_or("")))
                } else if let Some(pos) = r.find("/http/") {
                    Some(format!("http/{}", r[pos + 6..].split('/').next().unwrap_or("")))
                } else { None }
            });
        }
        ctx
    };

    let ctx_str = match context {
        Some(c) => c,
        None => {
            log::warn!("[PWAResource] Context lost for URI: {}", uri);
            return Ok(Response::builder().status(404).body("Missing Context".into())?);
        }
    };

    // 2. 还原原始 URL
    let (proto, domain_raw) = ctx_str.split_once('/').unwrap_or((&ctx_str, ""));
    let real_domain = domain_raw.replace(".port-", ":");
    
    // 清理请求路径：如果是带上下文标记的路径，去掉标记部分
    let marker = format!("/{}", ctx_str);
    let final_path = if uri_path.starts_with(&marker) {
        &uri_path[marker.len()..]
    } else {
        uri_path
    };
    
    let query = url_obj.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let original_url = format!("{}://{}{}{}", proto, real_domain, final_path, query);

    log::info!("[PWAResource] Proxy: {} -> {}", uri, original_url);

    // 3. 缓存处理
    let domain_name = real_domain.split(':').next().unwrap_or("unknown");
    let ext = Path::new(final_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let should_cache = CACHE_WHITELIST.contains(&ext.as_str());

    let app_data_dir = app.path().app_data_dir()?;
    let cache_dir = app_data_dir.join("pwa_cache").join(domain_name);
    let local_file_path = if final_path == "/" || final_path.is_empty() || final_path.ends_with('/') {
        cache_dir.join("index.html")
    } else {
        cache_dir.join(final_path.trim_start_matches('/'))
    };

    // 4. 加载资源
    let (content, remote_mime, status) = if should_cache && local_file_path.exists() && local_file_path.is_file() {
        match fs::read(&local_file_path) {
            Ok(c) => (c, None, 200),
            Err(_) => fetch_from_network(&original_url, should_cache, &local_file_path)?
        }
    } else {
        fetch_from_network(&original_url, should_cache, &local_file_path)?
    };

    // 5. 响应并注入 Cookie 保持上下文
    let mut response = build_response(&local_file_path, content, remote_mime, status)?;
    response.headers_mut().insert(
        "Set-Cookie",
        format!("pwa_context={}; Path=/; Max-Age=3600; SameSite=Lax", ctx_str).parse().unwrap()
    );
    Ok(response)
}

fn fetch_from_network(url: &str, should_cache: bool, cache_path: &PathBuf) -> Result<(Vec<u8>, Option<String>, u16), Box<dyn std::error::Error>> {
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
