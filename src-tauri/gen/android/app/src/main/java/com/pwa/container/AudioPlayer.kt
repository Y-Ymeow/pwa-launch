package com.pwa.container

import android.content.Context
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.common.PlaybackException

/**
 * Android 音频播放器 - 使用 ExoPlayer
 * 支持后台播放、进度获取、循环播放
 */
class AudioPlayer(private val context: Context) {
    private val TAG = "AudioPlayer"
    private var player: ExoPlayer? = null
    private var currentUrl: String = ""
    private var isLooping: Boolean = false
    
    data class PlaybackState(
        val currentUrl: String,
        val positionMs: Long,
        val durationMs: Long,
        val isPlaying: Boolean,
        val isPaused: Boolean
    )
    
    init {
        initializePlayer()
    }
    
    private fun initializePlayer() {
        player = ExoPlayer.Builder(context).build().apply {
            addListener(object : Player.Listener {
                override fun onPlaybackStateChanged(playbackState: Int) {
                    when (playbackState) {
                        Player.STATE_READY -> Log.d(TAG, "Player ready")
                        Player.STATE_ENDED -> Log.d(TAG, "Playback ended")
                        Player.STATE_BUFFERING -> Log.d(TAG, "Buffering...")
                        Player.STATE_IDLE -> Log.d(TAG, "Player idle")
                    }
                }
                
                override fun onPlayerError(error: PlaybackException) {
                    Log.e(TAG, "Player error: ${error.errorCodeName}")
                }
            })
        }
    }
    
    fun play(url: String): String {
        Log.d(TAG, "Playing: $url")
        
        currentUrl = url
        
        return try {
            player?.let {
                val mediaItem = MediaItem.fromUri(url)
                it.setMediaItem(mediaItem)
                it.repeatMode = if (isLooping) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
                it.prepare()
                it.play()
                "Playing"
            } ?: "Error: Player not initialized"
        } catch (e: Exception) {
            Log.e(TAG, "Failed to play: ${e.message}")
            "Error: ${e.message}"
        }
    }
    
    fun pause() {
        Log.d(TAG, "Pause")
        player?.pause()
    }
    
    fun resume() {
        Log.d(TAG, "Resume")
        player?.play()
    }
    
    fun stop() {
        Log.d(TAG, "Stop")
        player?.stop()
        currentUrl = ""
    }
    
    fun setVolume(volume: Float) {
        val vol = volume.coerceIn(0f, 1f)
        Log.d(TAG, "Set volume: $vol")
        player?.volume = vol
    }
    
    fun seekTo(positionMs: Long) {
        Log.d(TAG, "Seek to: $positionMs")
        player?.seekTo(positionMs)
    }
    
    fun getState(): PlaybackState {
        return player?.let {
            PlaybackState(
                currentUrl = currentUrl,
                positionMs = it.currentPosition,
                durationMs = if (it.duration >= 0) it.duration else 0,
                isPlaying = it.isPlaying,
                isPaused = !it.isPlaying && it.playbackState != Player.STATE_IDLE
            )
        } ?: PlaybackState("", 0, 0, false, true)
    }
    
    fun getPosition(): Long {
        return player?.currentPosition ?: 0
    }
    
    fun getDuration(): Long {
        return player?.duration?.let { if (it >= 0) it else 0 } ?: 0
    }
    
    fun getCurrentUrl(): String {
        return currentUrl
    }
    
    fun setLoop(loop: Boolean) {
        Log.d(TAG, "Set loop: $loop")
        isLooping = loop
        player?.repeatMode = if (loop) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
    }
    
    fun release() {
        player?.release()
        player = null
    }
}

/**
 * 单例对象供 Rust 通过 JNI 调用
 */
object AudioPlayerInstance {
    private var instance: AudioPlayer? = null
    
    fun init(context: Context) {
        if (instance == null) {
            instance = AudioPlayer(context.applicationContext)
        }
    }
    
    fun getInstance(): AudioPlayer? {
        return instance
    }
}