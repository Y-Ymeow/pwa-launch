/// Android 音频播放 - 通过 JNI 调用 ExoPlayer
#[cfg(target_os = "android")]
pub mod android {
    use jni::objects::JString;
    use jni::signature::JavaType;
    use jni::JNIEnv;
    use tauri::Runtime;

    fn get_env<R: Runtime>(app: &tauri::AppHandle<R>) -> JNIEnv {
        // 通过 Tauri 获取 JNI 环境
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            env
        }
    }

    /// 播放音频
    pub fn play<R: Runtime>(app: &tauri::AppHandle<R>, url: &str) -> String {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            let url_jstring = env.new_string(url).unwrap();
            
            let result = env.call_static_method(
                &cls,
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
                Err(e) => format!("JNI Error: {:?}", e),
            }
        }
    }

    /// 暂停
    pub fn pause<R: Runtime>(app: &tauri::AppHandle<R>) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "pause",
                "()V",
                &[],
            ).ok();
        }
    }

    /// 继续
    pub fn resume<R: Runtime>(app: &tauri::AppHandle<R>) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "resume",
                "()V",
                &[],
            ).ok();
        }
    }

    /// 停止
    pub fn stop<R: Runtime>(app: &tauri::AppHandle<R>) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "stop",
                "()V",
                &[],
            ).ok();
        }
    }

    /// 设置音量
    pub fn set_volume<R: Runtime>(app: &tauri::AppHandle<R>, volume: f32) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "setVolume",
                "(F)V",
                &[volume.into()],
            ).ok();
        }
    }

    /// 跳转到指定位置
    pub fn seek<R: Runtime>(app: &tauri::AppHandle<R>, position_ms: u64) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "seekTo",
                "(J)V",
                &[(position_ms as i64).into()],
            ).ok();
        }
    }

    /// 设置循环
    pub fn set_loop<R: Runtime>(app: &tauri::AppHandle<R>, loop_enabled: bool) {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            env.call_static_method(
                &cls,
                "setLoop",
                "(Z)V",
                &[loop_enabled.into()],
            ).ok();
        }
    }

    /// 获取当前位置
    pub fn get_position<R: Runtime>(app: &tauri::AppHandle<R>) -> u64 {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            let result = env.call_static_method(
                &cls,
                "getPosition",
                "()J",
                &[],
            );
            
            match result {
                Ok(val) => val.j().unwrap_or(0) as u64,
                Err(_) => 0,
            }
        }
    }

    /// 获取总时长
    pub fn get_duration<R: Runtime>(app: &tauri::AppHandle<R>) -> u64 {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            let result = env.call_static_method(
                &cls,
                "getDuration",
                "()J",
                &[],
            );
            
            match result {
                Ok(val) => val.j().unwrap_or(0) as u64,
                Err(_) => 0,
            }
        }
    }

    /// 获取当前 URL
    pub fn get_current_url<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            let result = env.call_static_method(
                &cls,
                "getCurrentUrl",
                "()Ljava/lang/String;",
                &[],
            );
            
            match result {
                Ok(obj) => {
                    let jstr: JString = obj.l().unwrap().into();
                    env.get_string(&jstr).unwrap().into()
                }
                Err(_) => String::new(),
            }
        }
    }

    /// 是否正在播放
    pub fn is_playing<R: Runtime>(app: &tauri::AppHandle<R>) -> bool {
        unsafe {
            let vm = tauri::platform::android::vm();
            let mut env = vm.attach_current_thread().unwrap();
            
            let cls = env.find_class("com/pwa/container/AudioPlayerBridge").unwrap();
            let result = env.call_static_method(
                &cls,
                "isPlaying",
                "()Z",
                &[],
            );
            
            match result {
                Ok(val) => val.z().unwrap_or(false),
                Err(_) => false,
            }
        }
    }
}
