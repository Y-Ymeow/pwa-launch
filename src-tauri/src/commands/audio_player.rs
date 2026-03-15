use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Runtime;

// Linux: 使用 MPV
#[cfg(not(target_os = "android"))]
use libmpv2::Mpv;

// 全局播放器实例
#[cfg(not(target_os = "android"))]
lazy_static::lazy_static! {
    static ref MPV_INSTANCE: Mutex<Option<Mpv>> = Mutex::new(None);
}

lazy_static::lazy_static! {
    static ref CURRENT_URL: Mutex<String> = Mutex::new(String::new());
}

// 设置 MPV 需要的 C locale (Linux only)
#[cfg(not(target_os = "android"))]
fn set_mpv_locale() {
    std::env::set_var("LC_NUMERIC", "C");
}

/// 初始化 MPV (Linux only)
#[cfg(not(target_os = "android"))]
fn init_mpv() -> Result<(), String> {
    set_mpv_locale();
    
    let mut guard = MPV_INSTANCE.lock().unwrap();
    if guard.is_none() {
        let mpv = Mpv::new().map_err(|e| format!("Failed to create MPV: {:?}", e))?;
        
        // 配置 MPV - 后台播放模式
        let _ = mpv.set_property("vo", "null");
        let _ = mpv.set_property("force-window", "no");
        let _ = mpv.set_property("terminal", "no");
        let _ = mpv.set_property("idle", "no");
        
        *guard = Some(mpv);
        log::info!("[MPV] Initialized with libmpv2");
    }
    Ok(())
}

/// 播放音频
#[tauri::command]
pub async fn audio_play<R: Runtime>(app: AppHandle<R>, url: String) -> Result<String, String> {
    // 处理本地文件路径
    let url = if url.starts_with('/') {
        format!("file://{}", url)
    } else {
        url
    };
    
    log::info!("[Audio] Playing: {}", url);

    #[cfg(not(target_os = "android"))]
    {
        // Linux: 使用 MPV
        init_mpv()?;
        
        let mpv = MPV_INSTANCE.lock().unwrap();
        if let Some(ref mpv) = *mpv {
            let _ = mpv.command("stop", &[]);
            mpv.command("loadfile", &[&url, "replace"])
                .map_err(|e| format!("Failed to load file: {:?}", e))?;
        }
    }
    
    #[cfg(target_os = "android")]
    {
        // Android: 通过 JNI 调用 ExoPlayer
        use crate::commands::android_audio::android;
        android::play(&app, &url);
    }
    
    if let Ok(mut url_guard) = CURRENT_URL.lock() {
        *url_guard = url;
    }

    Ok("Playing".to_string())
}

/// 暂停
#[tauri::command]
pub fn audio_pause<R: Runtime>(app: AppHandle<R>) {
    log::info!("[Audio] Pause");
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let _ = mpv.set_property("pause", true);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::pause(&app);
    }
}

/// 继续
#[tauri::command]
pub fn audio_resume<R: Runtime>(app: AppHandle<R>) {
    log::info!("[Audio] Resume");
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let _ = mpv.set_property("pause", false);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::resume(&app);
    }
}

/// 停止
#[tauri::command]
pub fn audio_stop<R: Runtime>(app: AppHandle<R>) {
    log::info!("[Audio] Stop");
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let _ = mpv.command("stop", &[]);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::stop(&app);
    }
    
    if let Ok(mut url_guard) = CURRENT_URL.lock() {
        *url_guard = String::new();
    }
}

/// 设置音量
#[tauri::command]
pub fn audio_set_volume<R: Runtime>(app: AppHandle<R>, volume: f32) {
    log::info!("[Audio] Set volume: {}", volume);
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let vol = volume.clamp(0.0, 1.0) * 100.0;
                let _ = mpv.set_property("volume", vol as i64);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::set_volume(&app, volume);
    }
}

/// 跳转到指定位置（毫秒）
#[tauri::command]
pub fn audio_seek<R: Runtime>(app: AppHandle<R>, position_ms: u64) {
    log::info!("[Audio] Seek to: {}ms", position_ms);
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let pos_sec = (position_ms / 1000) as f64;
                let _ = mpv.command("seek", &[&pos_sec.to_string(), "absolute"]);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::seek(&app, position_ms);
    }
}

/// 设置循环
#[tauri::command]
pub fn audio_set_loop<R: Runtime>(app: AppHandle<R>, loop_enabled: bool) {
    log::info!("[Audio] Set loop: {}", loop_enabled);
    
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let loop_str = if loop_enabled { "inf" } else { "no" };
                let _ = mpv.set_property("loop-file", loop_str);
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::set_loop(&app, loop_enabled);
    }
}

/// 获取播放状态
#[tauri::command]
pub fn audio_get_state<R: Runtime>(app: AppHandle<R>) -> AudioState {
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(guard) = MPV_INSTANCE.lock() {
            if let Some(ref mpv) = *guard {
                let position_ms: f64 = mpv.get_property("time-pos").unwrap_or(0.0) * 1000.0;
                let duration_ms: f64 = mpv.get_property("duration").unwrap_or(0.0) * 1000.0;
                let paused: bool = mpv.get_property("pause").unwrap_or(true);
                
                return AudioState {
                    current_url: CURRENT_URL.lock().unwrap().clone(),
                    position_ms: position_ms as u64,
                    duration_ms: duration_ms as u64,
                    is_playing: !paused,
                    is_paused: paused,
                };
            }
        }
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        return AudioState {
            current_url: android::get_current_url(&app),
            position_ms: android::get_position(&app),
            duration_ms: android::get_duration(&app),
            is_playing: android::is_playing(&app),
            is_paused: !android::is_playing(&app),
        };
    }
    
    #[allow(unreachable_code)]
    AudioState {
        current_url: String::new(),
        position_ms: 0,
        duration_ms: 0,
        is_playing: false,
        is_paused: true,
    }
}

/// 获取当前位置（毫秒）
#[tauri::command]
pub fn audio_get_position<R: Runtime>(app: AppHandle<R>) -> u64 {
    #[cfg(not(target_os = "android"))]
    {
        audio_get_state(app).position_ms
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::get_position(&app)
    }
}

/// 获取总时长（毫秒）
#[tauri::command]
pub fn audio_get_duration<R: Runtime>(app: AppHandle<R>) -> u64 {
    #[cfg(not(target_os = "android"))]
    {
        audio_get_state(app).duration_ms
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::get_duration(&app)
    }
}

/// 获取当前播放 URL
#[tauri::command]
pub fn audio_get_current_url<R: Runtime>(app: AppHandle<R>) -> String {
    #[cfg(not(target_os = "android"))]
    {
        let _ = &app;
        CURRENT_URL.lock().unwrap().clone()
    }
    
    #[cfg(target_os = "android")]
    {
        use crate::commands::android_audio::android;
        android::get_current_url(&app)
    }
}

/// 音频状态结构
#[derive(serde::Serialize)]
pub struct AudioState {
    pub current_url: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing: bool,
    pub is_paused: bool,
}
