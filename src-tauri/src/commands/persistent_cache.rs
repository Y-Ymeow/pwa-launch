/**
 * 持久化缓存模块
 * 
 * 特性：
 * 1. 完全独立于 WebView 缓存系统
 * 2. 存储在应用数据目录，不会被系统自动清理
 * 3. 只有通过特定 API 或清除按钮才能清除
 * 4. 支持文本和二进制数据
 * 5. 支持元数据（MIME类型、过期时间等）
 */

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use tauri::Manager;
use crate::commands::get_app_data_dir;
use crate::models::CommandResponse;

/// 缓存元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub mime_type: String,
    pub size: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: Option<i64>, // None 表示永不过期
    pub metadata: Option<HashMap<String, String>>,
}

/// 获取持久化缓存目录
fn get_cache_dir(app_id: &str, app_data_dir: &Path) -> PathBuf {
    get_app_data_dir(app_id, app_data_dir).join("persistent_cache")
}

/// 获取缓存元数据文件路径
fn get_metadata_path(app_id: &str, app_data_dir: &Path) -> PathBuf {
    get_cache_dir(app_id, app_data_dir).join("_metadata.json")
}

/// 获取缓存文件路径
fn get_cache_file_path(app_id: &str, key: &str, app_data_dir: &Path) -> PathBuf {
    // 对 key 进行哈希，避免特殊字符问题
    let safe_key = sanitize_key(key);
    get_cache_dir(app_id, app_data_dir).join(format!("{}.bin", safe_key))
}

/// 清理 key 中的特殊字符
fn sanitize_key(key: &str) -> String {
    // 使用 base64 编码确保文件名安全
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, key)
}

/// 读取缓存元数据
fn read_metadata(app_id: &str, app_data_dir: &Path) -> HashMap<String, CacheEntry> {
    let path = get_metadata_path(app_id, app_data_dir);
    if !path.exists() {
        return HashMap::new();
    }
    
    match fs::read_to_string(&path) {
        Ok(content) => {
            serde_json::from_str(&content).unwrap_or_default()
        }
        Err(_) => HashMap::new(),
    }
}

/// 写入缓存元数据
fn write_metadata(app_id: &str, app_data_dir: &Path, metadata: &HashMap<String, CacheEntry>) -> Result<(), String> {
    let path = get_metadata_path(app_id, app_data_dir);
    let json = serde_json::to_string_pretty(metadata)
        .map_err(|e| format!("序列化元数据失败: {}", e))?;
    fs::write(&path, json)
        .map_err(|e| format!("写入元数据失败: {}", e))
}

/// 确保缓存目录存在
fn ensure_cache_dir(app_id: &str, app_data_dir: &Path) -> Result<(), String> {
    let dir = get_cache_dir(app_id, app_data_dir);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("创建缓存目录失败: {}", e))
}

/// 获取当前时间戳
fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 设置缓存项
#[tauri::command]
pub async fn cache_set(
    app_id: String,
    key: String,
    data: String, // base64 编码的数据
    options: Option<CacheSetOptions>,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    ensure_cache_dir(&app_id, &app_data_dir)?;
    
    // 解码 base64 数据
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &data
    ).map_err(|e| format!("解码 base64 失败: {}", e))?;
    
    // 写入文件
    let file_path = get_cache_file_path(&app_id, &key, &app_data_dir);
    fs::write(&file_path, &bytes)
        .map_err(|e| format!("写入缓存文件失败: {}", e))?;
    
    // 更新元数据
    let mut metadata = read_metadata(&app_id, &app_data_dir);
    let now = now_timestamp();
    
    let entry = CacheEntry {
        key: key.clone(),
        mime_type: options.as_ref().and_then(|o| o.mime_type.clone()).unwrap_or_else(|| "application/octet-stream".to_string()),
        size: bytes.len() as u64,
        created_at: metadata.get(&key).map(|e| e.created_at).unwrap_or(now),
        updated_at: now,
        expires_at: options.and_then(|o| o.ttl.map(|ttl| now + ttl)),
        metadata: None,
    };
    
    metadata.insert(key.clone(), entry);
    write_metadata(&app_id, &app_data_dir, &metadata)?;
    
    log::info!("[PersistentCache] Set: app_id={}, key={}, size={} bytes", app_id, sanitize_key(&key), bytes.len());
    
    Ok(CommandResponse::success(true))
}

/// 设置缓存项选项
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CacheSetOptions {
    pub mime_type: Option<String>,
    pub ttl: Option<i64>, // 生存时间（秒），None 表示永不过期
    pub metadata: Option<HashMap<String, String>>,
}

/// 获取缓存项
#[tauri::command]
pub async fn cache_get(
    app_id: String,
    key: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<Option<CacheGetResult>>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let metadata = read_metadata(&app_id, &app_data_dir);
    
    // 检查元数据是否存在
    let entry = match metadata.get(&key) {
        Some(e) => e.clone(),
        None => return Ok(CommandResponse::success(None)),
    };
    
    // 检查是否过期
    if let Some(expires_at) = entry.expires_at {
        if now_timestamp() > expires_at {
            // 已过期，删除
            let _ = cache_delete_internal(&app_id, &key, &app_data_dir);
            return Ok(CommandResponse::success(None));
        }
    }
    
    // 读取文件
    let file_path = get_cache_file_path(&app_id, &key, &app_data_dir);
    let bytes = match fs::read(&file_path) {
        Ok(b) => b,
        Err(_) => {
            // 文件不存在，清理元数据
            let _ = cache_delete_internal(&app_id, &key, &app_data_dir);
            return Ok(CommandResponse::success(None));
        }
    };
    
    // 编码为 base64
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    
    let result = CacheGetResult {
        key: key.clone(),
        data,
        mime_type: entry.mime_type,
        size: entry.size,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        expires_at: entry.expires_at,
    };
    
    log::debug!("[PersistentCache] Get: app_id={}, key={}, size={} bytes", app_id, sanitize_key(&key), bytes.len());
    
    Ok(CommandResponse::success(Some(result)))
}

/// 获取缓存项结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheGetResult {
    pub key: String,
    pub data: String, // base64 编码
    pub mime_type: String,
    pub size: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: Option<i64>,
}

/// 删除缓存项（内部函数）
fn cache_delete_internal(app_id: &str, key: &str, app_data_dir: &Path) -> Result<(), String> {
    let file_path = get_cache_file_path(app_id, key, app_data_dir);
    if file_path.exists() {
        fs::remove_file(&file_path)
            .map_err(|e| format!("删除缓存文件失败: {}", e))?;
    }
    
    let mut metadata = read_metadata(app_id, app_data_dir);
    metadata.remove(key);
    write_metadata(app_id, app_data_dir, &metadata)?;
    
    Ok(())
}

/// 删除缓存项
#[tauri::command]
pub async fn cache_delete(
    app_id: String,
    key: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    cache_delete_internal(&app_id, &key, &app_data_dir)?;
    
    log::info!("[PersistentCache] Delete: app_id={}, key={}", app_id, sanitize_key(&key));
    
    Ok(CommandResponse::success(true))
}

/// 列出所有缓存项
#[tauri::command]
pub async fn cache_list(
    app_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<CacheEntry>>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let metadata = read_metadata(&app_id, &app_data_dir);
    let now = now_timestamp();
    
    // 过滤掉过期的项
    let entries: Vec<CacheEntry> = metadata
        .into_values()
        .filter(|entry| {
            if let Some(expires_at) = entry.expires_at {
                now <= expires_at
            } else {
                true
            }
        })
        .collect();
    
    log::debug!("[PersistentCache] List: app_id={}, count={}", app_id, entries.len());
    
    Ok(CommandResponse::success(entries))
}

/// 清除所有缓存
#[tauri::command]
pub async fn cache_clear(
    app_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let cache_dir = get_cache_dir(&app_id, &app_data_dir);
    
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .map_err(|e| format!("清除缓存目录失败: {}", e))?;
    }
    
    log::info!("[PersistentCache] Clear: app_id={}", app_id);
    
    Ok(CommandResponse::success(true))
}

/// 获取缓存统计信息
#[tauri::command]
pub async fn cache_stats(
    app_id: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<CacheStats>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let metadata = read_metadata(&app_id, &app_data_dir);
    let now = now_timestamp();
    
    let mut total_size: u64 = 0;
    let mut valid_count = 0;
    let mut expired_count = 0;
    
    for entry in metadata.values() {
        if let Some(expires_at) = entry.expires_at {
            if now > expires_at {
                expired_count += 1;
                continue;
            }
        }
        total_size += entry.size;
        valid_count += 1;
    }
    
    let stats = CacheStats {
        total_entries: valid_count + expired_count,
        valid_entries: valid_count,
        expired_entries: expired_count,
        total_size_bytes: total_size,
        total_size_mb: (total_size as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0,
    };
    
    Ok(CommandResponse::success(stats))
}

/// 缓存统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub total_size_bytes: u64,
    pub total_size_mb: f64,
}

/// 检查缓存是否存在且未过期
#[tauri::command]
pub async fn cache_exists(
    app_id: String,
    key: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app.path().app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let metadata = read_metadata(&app_id, &app_data_dir);
    
    match metadata.get(&key) {
        Some(entry) => {
            // 检查是否过期
            if let Some(expires_at) = entry.expires_at {
                if now_timestamp() > expires_at {
                    return Ok(CommandResponse::success(false));
                }
            }
            
            // 检查文件是否存在
            let file_path = get_cache_file_path(&app_id, &key, &app_data_dir);
            Ok(CommandResponse::success(file_path.exists()))
        }
        None => Ok(CommandResponse::success(false)),
    }
}
