use rusqlite::{Connection, Result, OptionalExtension};
use std::path::Path;
use std::path::PathBuf;

/// 获取应用特定的数据目录
pub fn get_app_data_dir(app_id: &str, base_dir: &Path) -> PathBuf {
    base_dir.join("pwa_data").join(app_id)
}

/// 获取应用数据库路径（用于官方 SQL 插件）
pub fn get_app_db_path(app_id: &str, base_dir: &Path) -> PathBuf {
    get_app_data_dir(app_id, base_dir).join(format!("{}.db", app_id))
}

/// 确保应用数据目录存在
pub fn ensure_app_data_dir(app_id: &str, base_dir: &Path) -> std::io::Result<PathBuf> {
    let dir = get_app_data_dir(app_id, base_dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 获取指定 app_id 和 domain 的 cookies（直接查数据库）
pub fn get_cookies_for_domain(
    conn: &Connection,
    app_id: &str,
    domain: &str,
) -> Result<std::collections::HashMap<String, String>> {
    let mut cookies = std::collections::HashMap::new();
    
    let mut stmt = conn.prepare(
        "SELECT name, value FROM cookies WHERE app_id = ?1 AND domain = ?2"
    )?;
    
    let rows = stmt.query_map(rusqlite::params![app_id, domain], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
        ))
    })?;
    
    for row in rows {
        let (name, value) = row?;
        cookies.insert(name, value);
    }
    
    Ok(cookies)
}

/// 获取所有有 cookies 的域名列表（按域名分组）
pub fn get_cookie_domains(
    conn: &Connection,
) -> Result<Vec<String>> {
    let mut domains = Vec::new();
    
    let mut stmt = conn.prepare(
        "SELECT DISTINCT domain FROM cookies ORDER BY domain"
    )?;
    
    let rows = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    })?;
    
    for row in rows {
        domains.push(row?);
    }
    
    Ok(domains)
}

/// 保存单个 cookie 到数据库
pub fn save_cookie(
    conn: &Connection,
    app_id: &str,
    domain: &str,
    name: &str,
    value: &str,
) -> Result<()> {
    // 确保 cookies 表存在
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cookies (
            app_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at INTEGER DEFAULT (strftime('%s', 'now')),
            PRIMARY KEY (app_id, domain, name)
        )",
        [],
    )?;
    
    conn.execute(
        "INSERT OR REPLACE INTO cookies (app_id, domain, name, value, updated_at) 
         VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))",
        rusqlite::params![app_id, domain, name, value],
    )?;
    Ok(())
}

/// 批量保存 cookies 到数据库（用于 Set-Cookie 响应）
pub fn save_cookies_batch(
    conn: &Connection,
    app_id: &str,
    domain: &str,
    cookies: &[(String, String)],
) -> Result<()> {
    // 确保 cookies 表存在
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cookies (
            app_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at INTEGER DEFAULT (strftime('%s', 'now')),
            PRIMARY KEY (app_id, domain, name)
        )",
        [],
    )?;
    
    let tx = conn.unchecked_transaction()?;
    
    for (name, value) in cookies {
        tx.execute(
            "INSERT OR REPLACE INTO cookies (app_id, domain, name, value, updated_at) 
             VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))",
            rusqlite::params![app_id, domain, name, value],
        )?;
    }
    
    tx.commit()?;
    log::debug!("[DB] Saved {} cookies for {}/{}", cookies.len(), app_id, domain);
    Ok(())
}

/// 获取配置项
pub fn get_config(conn: &Connection, key: &str) -> Result<Option<String>> {
    // 确保 app_config 表存在
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    
    let result: Option<String> = conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).optional()?;
    Ok(result)
}

/// 设置配置项
pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    // 确保 app_config 表存在
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    
    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    log::info!("[DB] Config updated: {} = {}", key, value);
    Ok(())
}

/// 获取全局 User-Agent（便捷函数）
pub fn get_user_agent(conn: &Connection) -> Result<String> {
    get_config(conn, "user_agent").map(|v| v.unwrap_or_else(|| 
        "Mozilla/5.0 (Linux; Android 13; TECNO BG6 Build/TP1A.220624.014) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.7632.159 Mobile Safari/537.36".to_string()
    ))
}

/// 设置全局 User-Agent（便捷函数）
pub fn set_user_agent(conn: &Connection, user_agent: &str) -> Result<()> {
    set_config(conn, "user_agent", user_agent)
}

/// 解析并保存 cookie 字符串 (格式: "key1=value1; key2=value2")
pub fn parse_and_save_cookie_string(
    conn: &Connection,
    app_id: &str,
    domain: &str,
    cookie_str: &str,
) -> Result<()> {
    // 确保 cookies 表存在
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cookies (
            app_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at INTEGER DEFAULT (strftime('%s', 'now')),
            PRIMARY KEY (app_id, domain, name)
        )",
        [],
    )?;
    
    let tx = conn.unchecked_transaction()?;
    
    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(eq_pos) = cookie.find('=') {
            let name = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !name.is_empty() {
                tx.execute(
                    "INSERT OR REPLACE INTO cookies (app_id, domain, name, value, updated_at) 
                     VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))",
                    rusqlite::params![app_id, domain, name, value],
                )?;
            }
        }
    }
    
    tx.commit()?;
    Ok(())
}

pub type DbConnection = std::sync::Mutex<Connection>;