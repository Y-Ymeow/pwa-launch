use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::Result;
use crate::AudioplayerExt;

#[command]
pub(crate) async fn play<R: Runtime>(
    app: AppHandle<R>,
    payload: PlayRequest,
) -> Result<PlayResponse> {
    app.audioplayer().play(payload)
}

#[command]
pub(crate) fn pause<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.audioplayer().pause()
}

#[command]
pub(crate) fn resume<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.audioplayer().resume()
}

#[command]
pub(crate) fn stop<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.audioplayer().stop()
}

#[command]
pub(crate) fn set_volume<R: Runtime>(app: AppHandle<R>, volume: f32) -> Result<()> {
    app.audioplayer().set_volume(VolumeRequest { volume })
}

#[command]
pub(crate) fn seek<R: Runtime>(app: AppHandle<R>, position_ms: u64) -> Result<()> {
    app.audioplayer().seek(SeekRequest { position_ms })
}

#[command]
pub(crate) fn set_loop<R: Runtime>(app: AppHandle<R>, loop_enabled: bool) -> Result<()> {
    app.audioplayer().set_loop(LoopRequest { loop_enabled })
}

#[command]
pub(crate) fn get_state<R: Runtime>(app: AppHandle<R>) -> Result<AudioState> {
    app.audioplayer().get_state()
}

#[command]
pub(crate) fn get_position<R: Runtime>(app: AppHandle<R>) -> Result<u64> {
    let resp = app.audioplayer().get_position()?;
    Ok(resp.position_ms)
}

#[command]
pub(crate) fn get_duration<R: Runtime>(app: AppHandle<R>) -> Result<u64> {
    let resp = app.audioplayer().get_duration()?;
    Ok(resp.duration_ms)
}

#[command]
pub(crate) fn get_current_url<R: Runtime>(app: AppHandle<R>) -> Result<String> {
    let resp = app.audioplayer().get_current_url()?;
    Ok(resp.url)
}