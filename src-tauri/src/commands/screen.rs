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
        let result = _app.run_on_main_thread(move || {
            let ctx = ndk_context::android_context();
            let vm_ptr = ctx.vm();
            let activity_ptr = ctx.context();
            
            // 使用 jni crate 的 JavaVM
            let vm: jni::JavaVM = unsafe { jni::JavaVM::from_raw(vm_ptr as _) }.expect("Failed to get JavaVM");
            let mut env = vm.attach_current_thread().expect("Failed to attach thread");
            let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr as _) };
            
            let method_name = if enabled { "setKeepScreenOn" } else { "clearKeepScreenOn" };
            let _ = env.call_method(
                &activity,
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
