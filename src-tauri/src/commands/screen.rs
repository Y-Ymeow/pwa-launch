use super::CommandResponse;

/// 设置屏幕常亮状态
#[tauri::command]
pub async fn set_keep_screen_on(
    _app: tauri::AppHandle,
    enabled: bool,
) -> Result<CommandResponse<bool>, String> {
    #[cfg(target_os = "android")]
    {
        // 通过 JNI 调用 Android 的 MainActivity.setKeepScreenOn
        use jni::objects::JObject;
        use jni::signature::JavaType;
        
        let result = _app.run_on_android_thread(move |env, activity| {
            let method_name = if enabled { "setKeepScreenOn" } else { "clearKeepScreenOn" };
            let _ = env.call_method(
                activity,
                method_name,
                "()V",
                &[],
            );
        });
        
        if let Err(e) = result {
            log::error!("Failed to set keep screen on: {}", e);
        }
    }
    
    // 桌面端：记录日志
    #[cfg(not(target_os = "android"))]
    {
        log::info!("Keep screen on (desktop): {}", enabled);
    }
    
    Ok(CommandResponse::success(enabled))
}

/// 获取屏幕常亮状态
#[tauri::command]
pub async fn get_keep_screen_on() -> Result<CommandResponse<bool>, String> {
    Ok(CommandResponse::success(false))
}
