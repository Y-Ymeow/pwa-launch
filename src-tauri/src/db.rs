use rusqlite::{Connection, Result, OptionalExtension};
use std::path::Path;

/// 获取应用特定的数据目录
pub fn get_app_data_dir(app_id: &str, base_dir: &Path) -> PathBuf {
    base_dir.join("pwa_data").join(app_id)
}

use std::path::PathBuf;

/// 初始化数据库
pub fn init_db(app_data_dir: &Path) -> Result<()> {
    let db_path = app_data_dir.join("pwa_container.db");

    let conn = Connection::open(&db_path)?;

    // 创建应用表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS apps (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            icon_url TEXT,
            manifest_url TEXT,
            installed_at INTEGER,
            updated_at INTEGER,
            start_url TEXT,
            scope TEXT,
            theme_color TEXT,
            background_color TEXT,
            display_mode TEXT
        )",
        [],
    )?;

    // 创建键值对存储表 (持久化 localStorage)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kv_store (
            app_id TEXT,
            key TEXT,
            value TEXT,
            PRIMARY KEY (app_id, key)
        )",
        [],
    )?;

    // 创建备份记录表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS backups (
            id TEXT PRIMARY KEY,
            app_id TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            size INTEGER NOT NULL,
            version TEXT
        )",
        [],
    )?;

    // 创建 cookies 表
    create_cookies_table(&conn)?;
    
    // 创建 app_config 表（通用配置）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    
    // 插入默认 User-Agent
    conn.execute(
        "INSERT OR IGNORE INTO app_config (key, value) VALUES ('user_agent', ?1)",
        ["Mozilla/5.0 (Linux; Android 13; TECNO BG6 Build/TP1A.220624.014) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.7632.159 Mobile Safari/537.36"],
    )?;

    Ok(())
}

/// 创建 cookies 表
fn create_cookies_table(conn: &Connection) -> Result<()> {
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
    
    // 创建索引
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cookies_app_domain ON cookies (app_id, domain)",
        [],
    )?;
    
    Ok(())
}

/// 保存 cookies 到数据库
pub fn save_cookies_to_db(
    conn: &Connection,
    cookies: &std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    
    // 清空现有 cookies
    tx.execute("DELETE FROM cookies", [])?;
    
    // 插入新 cookies
    for (app_id, app_cookies) in cookies {
        for (domain, domain_cookies) in app_cookies {
            for (name, value) in domain_cookies {
                tx.execute(
                    "INSERT INTO cookies (app_id, domain, name, value) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![app_id, domain, name, value],
                )?;
            }
        }
    }
    
    tx.commit()?;
    log::info!("[DB] Saved {} app cookies to database", cookies.len());
    Ok(())
}

/// 从数据库加载 cookies
pub fn load_cookies_from_db(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>>> {
    let mut cookies: std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>> = std::collections::HashMap::new();
    
    let mut stmt = conn.prepare(
        "SELECT app_id, domain, name, value FROM cookies"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    
    for row in rows {
        let (app_id, domain, name, value) = row?;
        let app_cookies = cookies.entry(app_id).or_insert_with(std::collections::HashMap::new);
        let domain_cookies = app_cookies.entry(domain).or_insert_with(std::collections::HashMap::new);
        domain_cookies.insert(name, value);
    }
    
    log::info!("[DB] Loaded cookies for {} apps from database", cookies.len());
    Ok(cookies)
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

/// 解析并保存 cookie 字符串 (格式: "key1=value1; key2=value2")
pub fn parse_and_save_cookie_string(
    conn: &Connection,
    app_id: &str,
    domain: &str,
    cookie_str: &str,
) -> Result<()> {
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

/// 获取配置项
pub fn get_config(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result: Option<String> = conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).optional()?;
    Ok(result)
}

/// 设置配置项
pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
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

pub type DbConnection = std::sync::Mutex<Connection>;
