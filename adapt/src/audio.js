/**
 * Audio API - 使用 Rust 后端播放音频（绕过 WebKitGTK GStreamer）
 * 支持播放进度获取
 */

// 音频播放器状态
let currentAudioUrl = null;
let isPlaying = false;
let progressInterval = null;

/**
 * 播放音频（使用 rodio 后端）
 * @param {string} url - 音频 URL
 */
export async function playAudio(url) {
  if (window.__TAURI__) {
    try {
      await window.__TAURI__.invoke("audio_play", { url });
      currentAudioUrl = url;
      isPlaying = true;
      startProgressTracking();
      return true;
    } catch (e) {
      console.error("[PWA Adapt Audio] Failed to play:", e);
      // 回退到原生 HTML5 Audio
      return playAudioNative(url);
    }
  } else {
    // 非 Tauri 环境，使用原生 Audio
    return playAudioNative(url);
  }
}

/**
 * 暂停播放
 */
export function pauseAudio() {
  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_pause");
    isPlaying = false;
  }

  // 同时停止原生音频
  if (nativeAudioElement) {
    nativeAudioElement.pause();
  }
}

/**
 * 继续播放
 */
export function resumeAudio() {
  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_resume");
    isPlaying = true;
    startProgressTracking();
  }

  if (nativeAudioElement) {
    nativeAudioElement.play();
  }
}

/**
 * 停止播放
 */
export function stopAudio() {
  stopProgressTracking();

  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_stop");
  }

  // 同时停止原生音频
  if (nativeAudioElement) {
    nativeAudioElement.pause();
    nativeAudioElement = null;
  }

  isPlaying = false;
  currentAudioUrl = null;
}

/**
 * 设置音量（0.0 - 1.0）
 */
export function setAudioVolume(volume) {
  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_set_volume", { volume });
  }

  if (nativeAudioElement) {
    nativeAudioElement.volume = volume;
  }
}

/**
 * 设置循环播放
 * @param {boolean} loop - 是否循环
 */
export function setAudioLoop(loop) {
  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_set_loop", { loopPlay: loop });
  }

  if (nativeAudioElement) {
    nativeAudioElement.loop = loop;
  }
}

/**
 * 获取播放状态
 * @returns {Promise<{currentUrl: string, positionMs: number, isPlaying: boolean, isPaused: boolean}>}
 */
export async function getAudioState() {
  if (window.__TAURI__) {
    try {
      return await window.__TAURI__.invoke("audio_get_state");
    } catch (e) {
      console.error("[PWA Adapt Audio] Failed to get state:", e);
    }
  }
  return {
    currentUrl: currentAudioUrl || "",
    positionMs: 0,
    isPlaying: false,
    isPaused: true,
  };
}

/**
 * 获取当前播放位置（毫秒）
 * @returns {Promise<number>}
 */
export async function getAudioPosition() {
  if (window.__TAURI__) {
    try {
      return await window.__TAURI__.invoke("audio_get_position");
    } catch (e) {
      console.error("[PWA Adapt Audio] Failed to get position:", e);
    }
  }
  return nativeAudioElement ? nativeAudioElement.currentTime * 1000 : 0;
}

/**
 * 获取当前播放的 URL
 * @returns {Promise<string>}
 */
export async function getAudioCurrentUrl() {
  if (window.__TAURI__) {
    try {
      return await window.__TAURI__.invoke("audio_get_current_url");
    } catch (e) {
      console.error("[PWA Adapt Audio] Failed to get current URL:", e);
    }
  }
  return currentAudioUrl || "";
}

/**
 * 获取音频总时长（毫秒）
 * @returns {Promise<number>}
 */
export async function getAudioDuration() {
  if (window.__TAURI__) {
    try {
      return await window.__TAURI__.invoke("audio_get_duration");
    } catch (e) {
      console.error("[PWA Adapt Audio] Failed to get duration:", e);
    }
  }
  return nativeAudioElement ? nativeAudioElement.duration * 1000 : 0;
}

/**
 * 跳转到指定位置（毫秒）
 * @param {number} positionMs - 目标位置（毫秒）
 */
export function seekAudio(positionMs) {
  console.log("[PWA Adapt Audio] Seek to:", positionMs);

  if (window.__TAURI__) {
    window.__TAURI__.invoke("audio_seek", { positionMs });
  }

  if (nativeAudioElement) {
    nativeAudioElement.currentTime = positionMs / 1000;
  }
}

/**
 * 播放视频（使用 MPV）
 * @param {string} url - 视频 URL
 */
export async function playVideo(url) {
  console.log("[PWA Adapt Video] Playing:", url);

  if (window.__TAURI__) {
    try {
      await window.__TAURI__.invoke("video_play", { url });
      return true;
    } catch (e) {
      console.error("[PWA Adapt Video] Failed to play:", e);
    }
  }

  // 回退：使用 video 标签
  const video = document.createElement("video");
  video.src = url;
  video.controls = true;
  video.style.position = "fixed";
  video.style.top = "0";
  video.style.left = "0";
  video.style.width = "100%";
  video.style.height = "100%";
  video.style.zIndex = "9999";
  document.body.appendChild(video);
  video.play();

  return true;
}

// 进度跟踪
let progressCallback = null;

function startProgressTracking() {
  stopProgressTracking();
  progressInterval = setInterval(async () => {
    if (progressCallback && window.__TAURI__) {
      const state = await getAudioState();
      progressCallback(state);
    }
  }, 500); // 每 500ms 更新一次
}

function stopProgressTracking() {
  if (progressInterval) {
    clearInterval(progressInterval);
    progressInterval = null;
  }
}

/**
 * 设置进度回调函数
 * @param {function} callback - 回调函数，接收 {currentUrl, positionMs, isPlaying, isPaused}
 */
export function setAudioProgressCallback(callback) {
  progressCallback = callback;
  if (callback && isPlaying) {
    startProgressTracking();
  } else {
    stopProgressTracking();
  }
}

// 原生 HTML5 Audio 元素（作为回退）
let nativeAudioElement = null;

/**
 * 使用原生 HTML5 Audio 播放（回退方案）
 */
function playAudioNative(url) {
  console.log("[PWA Adapt Audio] Using native HTML5 Audio:", url);

  // 停止之前的播放
  if (nativeAudioElement) {
    nativeAudioElement.pause();
    nativeAudioElement = null;
  }

  // 创建新的音频元素
  const audio = new Audio(url);
  audio.crossOrigin = "anonymous";

  audio.addEventListener("canplay", () => {
    console.log("[PWA Adapt Audio] Can play, starting...");
    audio.play().catch((e) => {
      console.error("[PWA Adapt Audio] Play failed:", e);
    });
  });

  audio.addEventListener("error", (e) => {
    console.error("[PWA Adapt Audio] Error:", e);
  });

  // 进度跟踪
  if (progressCallback) {
    audio.addEventListener("timeupdate", () => {
      progressCallback({
        currentUrl: url,
        positionMs: audio.currentTime * 1000,
        isPlaying: !audio.paused,
        isPaused: audio.paused,
      });
    });
  }

  nativeAudioElement = audio;
  currentAudioUrl = url;
  isPlaying = true;

  return true;
}

// 导出兼容的 Audio 类（类似 HTML5 Audio API）
export class AdaptAudio {
  constructor(url) {
    this.url = url;
    this.volume = 1.0;
    this.paused = true;
    this.ended = false;
    this.currentTime = 0;

    // 事件监听器
    this._listeners = {};
  }

  async play() {
    this.paused = false;
    return await playAudio(this.url);
  }

  pause() {
    this.paused = true;
    pauseAudio();
  }

  async getCurrentTime() {
    return await getAudioPosition();
  }

  addEventListener(event, callback) {
    if (!this._listeners[event]) {
      this._listeners[event] = [];
    }
    this._listeners[event].push(callback);
  }

  removeEventListener(event, callback) {
    if (this._listeners[event]) {
      this._listeners[event] = this._listeners[event].filter(
        (cb) => cb !== callback,
      );
    }
  }
}

