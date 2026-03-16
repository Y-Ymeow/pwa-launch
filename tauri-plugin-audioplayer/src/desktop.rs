use serde::de::DeserializeOwned;
use std::sync::Mutex;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

// Linux: 使用 MPV
use libmpv2::Mpv;

// 全局播放器实例
lazy_static::lazy_static! {
    static ref MPV_INSTANCE: Mutex<Option<Mpv>> = Mutex::new(None);
    static ref CURRENT_URL: Mutex<String> = Mutex::new(String::new());
}

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Audioplayer<R>> {
  // 设置 MPV 需要的 C locale
  std::env::set_var("LC_NUMERIC", "C");
  std::env::set_var("LC_ALL", "C");
  Ok(Audioplayer(app.clone()))
}

/// Access to the audioplayer APIs.
pub struct Audioplayer<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Audioplayer<R> {
  pub fn play(&self, payload: PlayRequest) -> crate::Result<PlayResponse> {
    let url = payload.url;
    log::info!("[AudioPlayer] Playing: {}", url);

    // 处理 URL
    let file_path = if url.starts_with("file://") {
        url[7..].to_string()
    } else {
        url
    };

    // 设置 C locale (MPV 要求)
    std::env::set_var("LC_NUMERIC", "C");
    std::env::set_var("LC_ALL", "C");
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, "C\0".as_ptr() as *const i8);
        libc::setlocale(libc::LC_ALL, "C\0".as_ptr() as *const i8);
    }

    // 销毁旧实例
    {
        let mut guard = MPV_INSTANCE.lock().unwrap();
        *guard = None;
    }

    // 创建新实例
    let mpv = Mpv::new().map_err(|e| crate::Error::MpvsError(e.to_string()))?;

    // 配置 MPV
    let _ = mpv.set_property("vo", "null");
    let _ = mpv.set_property("force-window", "no");
    let _ = mpv.set_property("terminal", "no");

    // 加载并播放文件
    mpv.command("loadfile", &[&file_path, "replace"])
        .map_err(|e| crate::Error::MpvsError(format!("Failed to load file: {:?}", e)))?;

    // 保存实例和 URL
    {
        let mut guard = MPV_INSTANCE.lock().unwrap();
        *guard = Some(mpv);
    }
    {
        let mut url_guard = CURRENT_URL.lock().unwrap();
        *url_guard = file_path;
    }

    Ok(PlayResponse { status: "Playing".to_string() })
  }

  pub fn pause(&self) -> crate::Result<()> {
    log::info!("[AudioPlayer] Pause");
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let _ = mpv.set_property("pause", true);
        }
    }
    Ok(())
  }

  pub fn resume(&self) -> crate::Result<()> {
    log::info!("[AudioPlayer] Resume");
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let _ = mpv.set_property("pause", false);
        }
    }
    Ok(())
  }

  pub fn stop(&self) -> crate::Result<()> {
    log::info!("[AudioPlayer] Stop");
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let _ = mpv.command("stop", &[]);
        }
    }
    if let Ok(mut url_guard) = CURRENT_URL.lock() {
        *url_guard = String::new();
    }
    Ok(())
  }

  pub fn set_volume(&self, payload: VolumeRequest) -> crate::Result<()> {
    let volume = payload.volume.clamp(0.0, 1.0);
    log::info!("[AudioPlayer] Set volume: {}", volume);
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let vol = volume * 100.0;
            let _ = mpv.set_property("volume", vol as i64);
        }
    }
    Ok(())
  }

  pub fn seek(&self, payload: SeekRequest) -> crate::Result<()> {
    log::info!("[AudioPlayer] Seek to: {}ms", payload.position_ms);
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let pos_sec = (payload.position_ms / 1000) as f64;
            let _ = mpv.command("seek", &[&pos_sec.to_string(), "absolute"]);
        }
    }
    Ok(())
  }

  pub fn set_loop(&self, payload: LoopRequest) -> crate::Result<()> {
    log::info!("[AudioPlayer] Set loop: {}", payload.loop_enabled);
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let loop_str = if payload.loop_enabled { "inf" } else { "no" };
            let _ = mpv.set_property("loop-file", loop_str);
        }
    }
    Ok(())
  }

  pub fn get_state(&self) -> crate::Result<AudioState> {
    if let Ok(guard) = MPV_INSTANCE.lock() {
        if let Some(ref mpv) = *guard {
            let position_ms: f64 = mpv.get_property("time-pos").unwrap_or(0.0) * 1000.0;
            let duration_ms: f64 = mpv.get_property("duration").unwrap_or(0.0) * 1000.0;
            let paused: bool = mpv.get_property("pause").unwrap_or(true);

            return Ok(AudioState {
                current_url: CURRENT_URL.lock().unwrap().clone(),
                position_ms: position_ms as u64,
                duration_ms: duration_ms as u64,
                is_playing: !paused,
                is_paused: paused,
            });
        }
    }
    Ok(AudioState {
        current_url: String::new(),
        position_ms: 0,
        duration_ms: 0,
        is_playing: false,
        is_paused: true,
    })
  }

  pub fn get_position(&self) -> crate::Result<PositionResponse> {
    let state = self.get_state()?;
    Ok(PositionResponse { position_ms: state.position_ms })
  }

  pub fn get_duration(&self) -> crate::Result<DurationResponse> {
    let state = self.get_state()?;
    Ok(DurationResponse { duration_ms: state.duration_ms })
  }

  pub fn get_current_url(&self) -> crate::Result<UrlResponse> {
    let url = CURRENT_URL.lock().unwrap().clone();
    Ok(UrlResponse { url })
  }
}