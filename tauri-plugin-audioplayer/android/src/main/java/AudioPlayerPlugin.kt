package com.plugin.audioplayer

import android.app.Activity
import android.net.Uri
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.PlaybackException
import androidx.media3.exoplayer.ExoPlayer
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke

@InvokeArg
class PlayArgs {
    var url: String? = null
}

@InvokeArg
class VolumeArgs {
    var volume: Float = 1.0f
}

@InvokeArg
class SeekArgs {
    var positionMs: Long = 0
}

@InvokeArg
class LoopArgs {
    var loop: Boolean = false
}

@TauriPlugin
class AudioPlayerPlugin(private val activity: Activity): Plugin(activity) {
    private val TAG = "AudioPlayerPlugin"
    private var player: ExoPlayer? = null
    private var currentUrl: String = ""
    private var isLooping: Boolean = false

    init {
        initializePlayer()
    }

    private fun initializePlayer() {
        player = ExoPlayer.Builder(activity).build().apply {
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

    @Command
    fun play(invoke: Invoke) {
        val args = invoke.parseArgs(PlayArgs::class.java)
        val url = args.url ?: ""

        Log.d(TAG, "Playing: $url")
        currentUrl = url

        try {
            player?.let {
                val mediaItem = if (url.startsWith("content://")) {
                    MediaItem.fromUri(Uri.parse(url))
                } else {
                    MediaItem.fromUri(url)
                }
                it.setMediaItem(mediaItem)
                it.repeatMode = if (isLooping) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
                it.prepare()
                it.play()

                val ret = JSObject()
                ret.put("status", "Playing")
                invoke.resolve(ret)
            } ?: run {
                invoke.reject("Player not initialized")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to play: ${e.message}", e)
            invoke.reject("Error: ${e.message}")
        }
    }

    @Command
    fun pause(invoke: Invoke) {
        Log.d(TAG, "Pause")
        player?.pause()
        invoke.resolve()
    }

    @Command
    fun resume(invoke: Invoke) {
        Log.d(TAG, "Resume")
        player?.play()
        invoke.resolve()
    }

    @Command
    fun stop(invoke: Invoke) {
        Log.d(TAG, "Stop")
        player?.stop()
        currentUrl = ""
        invoke.resolve()
    }

    @Command
    fun setVolume(invoke: Invoke) {
        val args = invoke.parseArgs(VolumeArgs::class.java)
        val vol = args.volume.coerceIn(0f, 1f)
        Log.d(TAG, "Set volume: $vol")
        player?.volume = vol
        invoke.resolve()
    }

    @Command
    fun seekTo(invoke: Invoke) {
        val args = invoke.parseArgs(SeekArgs::class.java)
        Log.d(TAG, "Seek to: ${args.positionMs}")
        player?.seekTo(args.positionMs)
        invoke.resolve()
    }

    @Command
    fun setLoop(invoke: Invoke) {
        val args = invoke.parseArgs(LoopArgs::class.java)
        isLooping = args.loop
        Log.d(TAG, "Set loop: $isLooping")
        player?.repeatMode = if (isLooping) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
        invoke.resolve()
    }

    @Command
    fun getState(invoke: Invoke) {
        val ret = JSObject()
        player?.let {
            ret.put("currentUrl", currentUrl)
            ret.put("positionMs", it.currentPosition)
            ret.put("durationMs", if (it.duration >= 0) it.duration else 0)
            ret.put("isPlaying", it.isPlaying)
            ret.put("isPaused", !it.isPlaying && it.playbackState != Player.STATE_IDLE)
        } ?: run {
            ret.put("currentUrl", "")
            ret.put("positionMs", 0)
            ret.put("durationMs", 0)
            ret.put("isPlaying", false)
            ret.put("isPaused", true)
        }
        invoke.resolve(ret)
    }

    @Command
    fun getPosition(invoke: Invoke) {
        val ret = JSObject()
        ret.put("positionMs", player?.currentPosition ?: 0)
        invoke.resolve(ret)
    }

    @Command
    fun getDuration(invoke: Invoke) {
        val ret = JSObject()
        ret.put("durationMs", player?.duration?.let { if (it >= 0) it else 0 } ?: 0)
        invoke.resolve(ret)
    }

    @Command
    fun getCurrentUrl(invoke: Invoke) {
        val ret = JSObject()
        ret.put("url", currentUrl)
        invoke.resolve(ret)
    }
}
