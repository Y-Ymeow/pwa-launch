/// Android 音频播放 - 通过 JNI 调用 ExoPlayer
#[cfg(target_os = "android")]
pub mod android {
    use jni::objects::JString;
    use jni::signature::JavaType;
    use jni::JNIEnv;
    use jni::JavaVM;
    use tauri::Runtime;
    use std::sync::OnceLock;
    
    static JVM: OnceLock<JavaVM> = OnceLock::new();
    const CLASS_NAME: &str = "com/pwa/container/AudioPlayerBridge";
    
    /// 初始化 JVM（在主线程调用一次）
    pub fn init_jvm() {
        unsafe {
            let ctx = ndk_context::android_context();
            let vm_ptr = ctx.vm();
            if !vm_ptr.is_null() {
                let vm = JavaVM::from_raw(vm_ptr as *mut _).ok();
                if let Some(vm) = vm {
                    let _ = JVM.set(vm);
                }
            }
        }
    }
    
    fn with_env<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&mut JNIEnv) -> R,
    {
        // 尝试从缓存的 JVM 获取
        if let Some(vm) = JVM.get() {
            if let Ok(mut env) = vm.attach_current_thread() {
                return Some(f(&mut env));
            }
        }
        
        // Fallback: 尝试从 ndk_context 获取
        unsafe {
            let ctx = ndk_context::android_context();
            let vm_ptr = ctx.vm();
            if !vm_ptr.is_null() {
                if let Ok(vm) = JavaVM::from_raw(vm_ptr as *mut _) {
                    if let Ok(mut env) = vm.attach_current_thread() {
                        return Some(f(&mut env));
                    }
                }
            }
        }
        
        log::error!("[AndroidAudio] Failed to get JNI environment");
        None
    }

    /// 播放音频
    pub fn play<R: Runtime>(_app: &tauri::AppHandle<R>, url: &str) -> String {
        log::info!("[AndroidAudio] play: {}", url);
        with_env(|env| {
            let url_jstring = match env.new_string(url) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("[AndroidAudio] Cannot create Java string: {:?}", e);
                    return "Error: Cannot create Java string".to_string();
                }
            };
            
            // 直接使用字符串类名调用，不先 find_class
            let result = env.call_static_method(
                CLASS_NAME,
                "play",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[(&url_jstring).into()],
            );
            
            match result {
                Ok(obj) => {
                    let jstr: JString = obj.l().unwrap().into();
                    let s: String = env.get_string(&jstr).unwrap().into();
                    s
                }
                Err(e) => {
                    log::error!("[AndroidAudio] JNI call failed: {:?}", e);
                    format!("JNI Error: {:?}", e)
                }
            }
        }).unwrap_or_else(|| {
            log::error!("[AndroidAudio] JNI environment not available");
            "Error: JNI environment not available".to_string()
        })
    }

    /// 暂停
    pub fn pause<R: Runtime>(_app: &tauri::AppHandle<R>) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "pause", "()V", &[]);
        });
    }

    /// 继续
    pub fn resume<R: Runtime>(_app: &tauri::AppHandle<R>) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "resume", "()V", &[]);
        });
    }

    /// 停止
    pub fn stop<R: Runtime>(_app: &tauri::AppHandle<R>) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "stop", "()V", &[]);
        });
    }

    /// 设置音量
    pub fn set_volume<R: Runtime>(_app: &tauri::AppHandle<R>, volume: f32) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "setVolume", "(F)V", &[volume.into()]);
        });
    }

    /// 跳转到指定位置
    pub fn seek<R: Runtime>(_app: &tauri::AppHandle<R>, position_ms: u64) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "seekTo", "(J)V", &[(position_ms as i64).into()]);
        });
    }

    /// 设置循环
    pub fn set_loop<R: Runtime>(_app: &tauri::AppHandle<R>, loop_enabled: bool) {
        let _ = with_env(|env| {
            let _ = env.call_static_method(CLASS_NAME, "setLoop", "(Z)V", &[loop_enabled.into()]);
        });
    }

    /// 获取当前位置
    pub fn get_position<R: Runtime>(_app: &tauri::AppHandle<R>) -> u64 {
        with_env(|env| {
            let result = env.call_static_method(CLASS_NAME, "getPosition", "()J", &[]);
            if let Ok(val) = result {
                if let Ok(long_val) = val.j() {
                    return long_val as u64;
                }
            }
            0
        }).unwrap_or(0)
    }

    /// 获取总时长
    pub fn get_duration<R: Runtime>(_app: &tauri::AppHandle<R>) -> u64 {
        with_env(|env| {
            let result = env.call_static_method(CLASS_NAME, "getDuration", "()J", &[]);
            if let Ok(val) = result {
                if let Ok(long_val) = val.j() {
                    return long_val as u64;
                }
            }
            0
        }).unwrap_or(0)
    }

    /// 获取当前 URL
    pub fn get_current_url<R: Runtime>(_app: &tauri::AppHandle<R>) -> String {
        with_env(|env| {
            let result = env.call_static_method(CLASS_NAME, "getCurrentUrl", "()Ljava/lang/String;", &[]);
            if let Ok(obj) = result {
                let jstr: JString = obj.l().unwrap().into();
                return env.get_string(&jstr).unwrap().into();
            }
            String::new()
        }).unwrap_or_default()
    }

    /// 是否正在播放
    pub fn is_playing<R: Runtime>(_app: &tauri::AppHandle<R>) -> bool {
        with_env(|env| {
            let result = env.call_static_method(CLASS_NAME, "isPlaying", "()Z", &[]);
            if let Ok(val) = result {
                if let Ok(bool_val) = val.z() {
                    return bool_val;
                }
            }
            false
        }).unwrap_or(false)
    }
}