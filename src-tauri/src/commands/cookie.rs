use super::{extract_domain, CommandResponse};

/// 读取 Cookie - 直接查数据库
#[tauri::command]
pub async fn get_cookies(
    url: String,
    app_id: String,
) -> Result<CommandResponse<Vec<String>>, String> {
    let domain = extract_domain(&url);
    
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    match crate::db::get_cookies_for_domain(&conn, &app_id, &domain) {
        Ok(cookies) => {
            let result: Vec<String> = cookies.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            Ok(CommandResponse::success(result))
        }
        Err(e) => {
            log::error!("[Cookies] Failed to get from DB: {}", e);
            Ok(CommandResponse::success(vec![]))
        }
    }
}

/// 设置 Cookie - 直接保存到数据库
#[tauri::command]
pub async fn set_cookies(
    url: String,
    app_id: String,
    cookies: Vec<String>,
) -> Result<CommandResponse<bool>, String> {
    let domain = extract_domain(&url);
    
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    for cookie in cookies {
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                if let Err(e) = crate::db::save_cookie(&conn, &app_id, &domain, &key, &value) {
                    log::error!("[Cookies] Failed to save cookie: {}", e);
                }
            }
        }
    }

    log::info!("[Cookies] Set cookies for app: {}, domain: {}", app_id, domain);
    Ok(CommandResponse::success(true))
}

/// 清除指定 app 的 Cookie（直接从数据库删除）
#[tauri::command]
pub async fn clear_cookies(
    app_id: String,
    domain: Option<String>,
    include_subdomains: Option<bool>,
) -> Result<CommandResponse<bool>, String> {
    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };

    if let Some(d) = domain {
        let include_subs = include_subdomains.unwrap_or(true);
        if include_subs {
            // 清除该域名及其子域
            // 例如 domain=manhwa-raw.com 会清除:
            // - manhwa-raw.com
            // - www.manhwa-raw.com
            // - xxx.manhwa-raw.com
            let like_pattern = format!("%.{}", d);
            conn.execute(
                "DELETE FROM cookies WHERE app_id = ?1 AND (domain = ?2 OR domain LIKE ?3)",
                rusqlite::params![app_id, d, like_pattern],
            ).map_err(|e| e.to_string())?;
            log::info!("清除 Cookie (app: {}, domain: {} 及子域)", app_id, d);
        } else {
            conn.execute(
                "DELETE FROM cookies WHERE app_id = ?1 AND domain = ?2",
                rusqlite::params![app_id, d],
            ).map_err(|e| e.to_string())?;
            log::info!("清除 Cookie (app: {}, domain: {})", app_id, d);
        }
    } else {
        conn.execute(
            "DELETE FROM cookies WHERE app_id = ?1",
            rusqlite::params![app_id],
        ).map_err(|e| e.to_string())?;
        log::info!("清除所有 Cookie (app: {})", app_id);
    }
    Ok(CommandResponse::success(true))
}

/// 从指定 WebView 获取 Cookies（包括 HttpOnly）
#[tauri::command]
pub fn get_webview_cookies(
    window: tauri::WebviewWindow,
) -> Result<CommandResponse<String>, String> {
    // 使用 WebView 的 cookies() API 获取所有 cookies（包括 HttpOnly）
    let cookies = window.cookies()
        .map_err(|e| format!("获取 cookies 失败: {:?}", e))?;
    
    // 将 cookies 转换为字符串格式
    let cookie_str = cookies.iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
    
    log::info!("从 WebView 获取到 {} 个 cookies", cookies.len());
    Ok(CommandResponse::success(cookie_str))
}

/// 从 WebView 同步 Cookies（直接保存到数据库）
#[tauri::command]
pub async fn sync_webview_cookies(
    domain: String,
    cookies: String,
    user_agent: Option<String>,
) -> Result<CommandResponse<bool>, String> {
    log::info!("同步 WebView Cookies for domain: {}", domain);
    log::info!("User-Agent: {:?}", user_agent);

    // 使用全局 DB_CONN
    let conn = if let Some(db_mutex) = crate::DB_CONN.get() {
        db_mutex.lock().map_err(|e| e.to_string())?
    } else {
        return Err("DB not initialized".to_string());
    };
    
    let mut count = 0;

    // 解析 cookies 字符串 (格式: "key1=value1; key2=value2")
    for cookie in cookies.split(';') {
        let cookie = cookie.trim();
        if let Some(eq_pos) = cookie.find('=') {
            let key = cookie[..eq_pos].trim().to_string();
            let value = cookie[eq_pos + 1..].trim().to_string();
            if !key.is_empty() {
                if let Err(e) = crate::db::save_cookie(&conn, "webview", &domain, &key, &value) {
                    log::error!("[Cookies] Failed to save: {}", e);
                } else {
                    count += 1;
                }
            }
        }
    }

    log::info!("WebView Cookies 同步完成: {} 个 cookies", count);
    Ok(CommandResponse::success(true))
}

/// 使用 JNI 从 Android WebView 获取 Cookies（包括 HttpOnly）
#[tauri::command]
pub async fn get_android_webview_cookies(url: String) -> Result<CommandResponse<String>, String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JString;
        use jni::signature::JavaType;
        use jni::strings::JavaStr;
        
        let url_for_log = url.clone();
        let result = tokio::task::spawn_blocking(move || {
            let ctx = ndk_context::android_context();
            let vm_ptr = ctx.vm();
            
            let vm: jni::JavaVM = unsafe { jni::JavaVM::from_raw(vm_ptr as _) }
                .map_err(|e| format!("Failed to get JavaVM: {:?}", e))?;
            let mut env = vm.attach_current_thread()
                .map_err(|e| format!("Failed to attach thread: {:?}", e))?;
            
            // 获取 CookieManager 类
            let cookie_manager_class = env.find_class("android/webkit/CookieManager")
                .map_err(|e| format!("Failed to find CookieManager class: {:?}", e))?;
            
            // 调用 CookieManager.getInstance()
            let instance = env.call_static_method(
                &cookie_manager_class,
                "getInstance",
                "()Landroid/webkit/CookieManager;",
                &[],
            ).map_err(|e| format!("Failed to get CookieManager instance: {:?}", e))?;
            
            let cookie_manager = instance.l()
                .map_err(|e| format!("Failed to convert to object: {:?}", e))?;
            
            // 调用 getCookie(url)
            let url_jstring = env.new_string(&url)
                .map_err(|e| format!("Failed to create Java string: {:?}", e))?;
            
            let cookie_result = env.call_method(
                &cookie_manager,
                "getCookie",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[(&url_jstring).into()],
            ).map_err(|e| format!("Failed to get cookie: {:?}", e))?;
            
            // 将 Java 字符串转换为 Rust 字符串
            let cookie_jstring: JString = cookie_result.l()
                .map_err(|e| format!("Failed to convert result to string: {:?}", e))?
                .into();
            
            let cookie_str = if cookie_jstring.is_null() {
                String::new()
            } else {
                let java_str = JavaStr::from_env(&env, &cookie_jstring)
                    .map_err(|e| format!("Failed to create JavaStr: {:?}", e))?;
                java_str.to_string_lossy().to_string()
            };
            
            Ok::<String, String>(cookie_str)
        }).await.map_err(|e| format!("Task join error: {:?}", e))?;
        
        match result {
            Ok(cookies) => {
                log::info!("[Android Cookies] Got {} bytes of cookies for {}", cookies.len(), url_for_log);
                Ok(CommandResponse::success(cookies))
            }
            Err(e) => {
                log::error!("[Android Cookies] Failed: {}", e);
                Err(e)
            }
        }
    }
    
    #[cfg(not(target_os = "android"))]
    {
        // 非 Android 平台返回空
        Ok(CommandResponse::success(String::new()))
    }
}
