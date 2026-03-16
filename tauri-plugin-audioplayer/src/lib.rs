use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Audioplayer;
#[cfg(mobile)]
use mobile::Audioplayer;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the audioplayer APIs.
pub trait AudioplayerExt<R: Runtime> {
  fn audioplayer(&self) -> &Audioplayer<R>;
}

impl<R: Runtime, T: Manager<R>> crate::AudioplayerExt<R> for T {
  fn audioplayer(&self) -> &Audioplayer<R> {
    self.state::<Audioplayer<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("audioplayer")
    .invoke_handler(tauri::generate_handler![
        commands::play,
        commands::pause,
        commands::resume,
        commands::stop,
        commands::set_volume,
        commands::seek,
        commands::set_loop,
        commands::get_state,
        commands::get_position,
        commands::get_duration,
        commands::get_current_url,
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let audioplayer = mobile::init(app, api)?;
      #[cfg(desktop)]
      let audioplayer = desktop::init(app, api)?;
      app.manage(audioplayer);
      Ok(())
    })
    .build()
}