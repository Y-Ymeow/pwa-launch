import { invoke } from '@tauri-apps/api/core';

export interface AudioState {
  currentUrl: string;
  positionMs: number;
  durationMs: number;
  isPlaying: boolean;
  isPaused: boolean;
}

/**
 * Play audio from URL
 * @param url - Audio file URL (http, https, or file://)
 * @returns Status string
 */
export async function play(url: string): Promise<string> {
  return await invoke<{ status: string }>('plugin:audioplayer|play', {
    payload: { url },
  }).then((r) => r.status);
}

/**
 * Pause playback
 */
export async function pause(): Promise<void> {
  await invoke('plugin:audioplayer|pause');
}

/**
 * Resume playback
 */
export async function resume(): Promise<void> {
  await invoke('plugin:audioplayer|resume');
}

/**
 * Stop playback
 */
export async function stop(): Promise<void> {
  await invoke('plugin:audioplayer|stop');
}

/**
 * Set volume (0.0 - 1.0)
 * @param volume - Volume level from 0.0 to 1.0
 */
export async function setVolume(volume: number): Promise<void> {
  await invoke('plugin:audioplayer|setVolume', { volume });
}

/**
 * Seek to position
 * @param positionMs - Position in milliseconds
 */
export async function seek(positionMs: number): Promise<void> {
  await invoke('plugin:audioplayer|seek', { positionMs });
}

/**
 * Set loop mode
 * @param loopEnabled - Enable or disable loop
 */
export async function setLoop(loopEnabled: boolean): Promise<void> {
  await invoke('plugin:audioplayer|setLoop', { loopEnabled });
}

/**
 * Get current playback state
 * @returns Current audio state
 */
export async function getState(): Promise<AudioState> {
  return await invoke<AudioState>('plugin:audioplayer|getState');
}

/**
 * Get current position
 * @returns Position in milliseconds
 */
export async function getPosition(): Promise<number> {
  return await invoke<number>('plugin:audioplayer|getPosition');
}

/**
 * Get total duration
 * @returns Duration in milliseconds
 */
export async function getDuration(): Promise<number> {
  return await invoke<number>('plugin:audioplayer|getDuration');
}

/**
 * Get current playing URL
 * @returns Current URL string
 */
export async function getCurrentUrl(): Promise<string> {
  return await invoke<string>('plugin:audioplayer|getCurrentUrl');
}