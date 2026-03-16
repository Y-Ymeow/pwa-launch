package com.plugin.audioplayer

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.util.Log
import android.os.Build
import androidx.core.content.FileProvider
import java.io.File
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
        var url = args.url ?: ""

        Log.d(TAG, "Playing: $url")

        try {
            player?.let {
                // 将本地文件路径转换为 content:// URI
                val uri = when {
                    url.startsWith("content://") -> {
                        val contentUri = Uri.parse(url)
                        // 尝试持久化 URI 权限（针对外部存储的 content URI）
                        try {
                            activity.contentResolver.takePersistableUriPermission(
                                contentUri,
                                Intent.FLAG_GRANT_READ_URI_PERMISSION
                            )
                            Log.d(TAG, "Persisted URI permission for: $url")
                        } catch (e: Exception) {
                            // 不是持久化的 URI，可能只是临时权限，继续尝试播放
                            Log.d(TAG, "Could not persist URI permission, may be temporary: ${e.message}")
                        }
                        contentUri
                    }
                    url.startsWith("/") -> {
                        // 本地文件路径，使用 FileProvider
                        val file = File(url)
                        FileProvider.getUriForFile(
                            activity,
                            "${activity.packageName}.fileprovider",
                            file
                        )
                    }
                    else -> Uri.parse(url)
                }

                currentUrl = url
                val mediaItem = MediaItem.fromUri(uri)

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
