use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_audioplayer);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Audioplayer<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.plugin.audioplayer", "AudioPlayerPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_audioplayer)?;
  Ok(Audioplayer(handle))
}

/// Access to the audioplayer APIs.
pub struct Audioplayer<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Audioplayer<R> {
  pub fn play(&self, payload: PlayRequest) -> crate::Result<PlayResponse> {
    self
      .0
      .run_mobile_plugin("play", payload)
      .map_err(Into::into)
  }

  pub fn pause(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("pause", ())
      .map_err(Into::into)
  }

  pub fn resume(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("resume", ())
      .map_err(Into::into)
  }

  pub fn stop(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("stop", ())
      .map_err(Into::into)
  }

  pub fn set_volume(&self, payload: VolumeRequest) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("setVolume", payload)
      .map_err(Into::into)
  }

  pub fn seek(&self, payload: SeekRequest) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("seekTo", payload)
      .map_err(Into::into)
  }

  pub fn set_loop(&self, payload: LoopRequest) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin("setLoop", payload)
      .map_err(Into::into)
  }

  pub fn get_state(&self) -> crate::Result<AudioState> {
    self
      .0
      .run_mobile_plugin("getState", ())
      .map_err(Into::into)
  }

  pub fn get_position(&self) -> crate::Result<PositionResponse> {
    self
      .0
      .run_mobile_plugin("getPosition", ())
      .map_err(Into::into)
  }

  pub fn get_duration(&self) -> crate::Result<DurationResponse> {
    self
      .0
      .run_mobile_plugin("getDuration", ())
      .map_err(Into::into)
  }

  pub fn get_current_url(&self) -> crate::Result<UrlResponse> {
    self
      .0
      .run_mobile_plugin("getCurrentUrl", ())
      .map_err(Into::into)
  }
}