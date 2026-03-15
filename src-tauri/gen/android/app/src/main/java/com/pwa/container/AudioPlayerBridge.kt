package com.pwa.container

import android.util.Log

/**
 * JNI 桥接 - 供 Rust 调用 ExoPlayer
 * 对应 Rust 中的 android_* 函数
 */
object AudioPlayerBridge {
    private const val TAG = "AudioPlayerBridge"
    
    @JvmStatic
    fun play(url: String): String {
        Log.d(TAG, "JNI play: $url")
        return AudioPlayerInstance.getInstance()?.play(url) ?: "Error: Not initialized"
    }
    
    @JvmStatic
    fun pause() {
        Log.d(TAG, "JNI pause")
        AudioPlayerInstance.getInstance()?.pause()
    }
    
    @JvmStatic
    fun resume() {
        Log.d(TAG, "JNI resume")
        AudioPlayerInstance.getInstance()?.resume()
    }
    
    @JvmStatic
    fun stop() {
        Log.d(TAG, "JNI stop")
        AudioPlayerInstance.getInstance()?.stop()
    }
    
    @JvmStatic
    fun setVolume(volume: Float) {
        Log.d(TAG, "JNI setVolume: $volume")
        AudioPlayerInstance.getInstance()?.setVolume(volume)
    }
    
    @JvmStatic
    fun seekTo(positionMs: Long) {
        Log.d(TAG, "JNI seekTo: $positionMs")
        AudioPlayerInstance.getInstance()?.seekTo(positionMs)
    }
    
    @JvmStatic
    fun setLoop(loop: Boolean) {
        Log.d(TAG, "JNI setLoop: $loop")
        AudioPlayerInstance.getInstance()?.setLoop(loop)
    }
    
    @JvmStatic
    fun getPosition(): Long {
        return AudioPlayerInstance.getInstance()?.getPosition() ?: 0
    }
    
    @JvmStatic
    fun getDuration(): Long {
        return AudioPlayerInstance.getInstance()?.getDuration() ?: 0
    }
    
    @JvmStatic
    fun getCurrentUrl(): String {
        return AudioPlayerInstance.getInstance()?.getCurrentUrl() ?: ""
    }
    
    @JvmStatic
    fun isPlaying(): Boolean {
        val state = AudioPlayerInstance.getInstance()?.getState()
        return state?.isPlaying ?: false
    }
}
