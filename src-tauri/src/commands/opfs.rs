use tauri::Manager;

use super::get_app_data_dir;
use crate::models::CommandResponse;

/// OPFS 写入文件
#[tauri::command]
pub async fn opfs_write_file(
    app_id: String,
    path: String,
    data: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    std::fs::create_dir_all(&files_dir).ok();

    let file_path = files_dir.join(&path);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let decoded = base64_decode(&data);
    std::fs::write(&file_path, decoded).map_err(|e| format!("写入失败：{}", e))?;

    log::info!("OPFS 写入：{}/{}", app_id, path);
    Ok(CommandResponse::success(true))
}

/// OPFS 读取文件
#[tauri::command]
pub async fn opfs_read_file(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let file_path = files_dir.join(&path);

    let data = std::fs::read(&file_path).map_err(|e| format!("读取失败：{}", e))?;

    Ok(CommandResponse::success(base64_encode(&data)))
}

/// OPFS 删除文件
#[tauri::command]
pub async fn opfs_delete_file(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<bool>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let file_path = files_dir.join(&path);

    std::fs::remove_file(&file_path).map_err(|e| format!("删除失败：{}", e))?;

    log::info!("OPFS 删除：{}/{}", app_id, path);
    Ok(CommandResponse::success(true))
}

/// OPFS 列出目录
#[tauri::command]
pub async fn opfs_list_dir(
    app_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<CommandResponse<Vec<String>>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    let files_dir = get_app_data_dir(&app_id, &app_data_dir).join("opfs");
    let dir_path = files_dir.join(&path);

    let entries = std::fs::read_dir(&dir_path).map_err(|e| format!("读取目录失败：{}", e))?;

    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    Ok(CommandResponse::success(names))
}

// Base64 编码/解码辅助函数
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        let _ = write!(result, "{}", ALPHABET[(b0 >> 2) & 0x3F] as char);
        let _ = write!(
            result,
            "{}",
            ALPHABET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char
        );
        if chunk.len() > 1 {
            let _ = write!(
                result,
                "{}",
                ALPHABET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char
            );
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            let _ = write!(result, "{}", ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(data: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for c in data.chars() {
        if c == '=' {
            break;
        }
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => continue,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
        }
    }
    result
}
