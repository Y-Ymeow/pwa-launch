use anyhow::Result;
use std::path::Path;
use std::fs;

/// 创建应用数据目录结构
pub fn create_app_dirs(app_id: &str, base_dir: &Path) -> Result<()> {
    let app_dir = base_dir.join("apps").join(app_id);
    fs::create_dir_all(&app_dir)?;
    fs::create_dir_all(app_dir.join("files"))?;
    fs::create_dir_all(app_dir.join("cache"))?;
    Ok(())
}

/// 删除应用数据目录
pub fn remove_app_dirs(app_id: &str, base_dir: &Path) -> Result<()> {
    let app_dir = base_dir.join("apps").join(app_id);
    if app_dir.exists() {
        fs::remove_dir_all(&app_dir)?;
    }
    Ok(())
}

/// 计算目录大小
pub fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total_size = 0u64;
    
    if path.exists() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += calculate_dir_size(&entry.path())?;
            }
        }
    }
    
    Ok(total_size)
}

/// 生成应用 ID
pub fn generate_app_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 获取当前时间戳
pub fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}
