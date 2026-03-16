package com.pwa.container

import android.os.Handler
import android.os.Looper
import android.util.Log
import java.util.concurrent.CountDownLatch

/**
 * JNI 桥接 - 供 Rust 调用 ExoPlayer
 * 对应 Rust 中的 android_* 函数
 * 所有操作在主线程执行，因为 ExoPlayer 必须在主线程访问
 */
object AudioPlayerBridge {
    private const val TAG = "AudioPlayerBridge"
    private val mainHandler = Handler(Looper.getMainLooper())
    
    @JvmStatic
    fun play(url: String): String {
        Log.d(TAG, "JNI play: $url")
        return runOnMainThread {
            AudioPlayerInstance.getInstance()?.play(url) ?: "Error: Not initialized"
        }
    }
    
    @JvmStatic
    fun pause() {
        Log.d(TAG, "JNI pause")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.pause()
        }
    }
    
    @JvmStatic
    fun resume() {
        Log.d(TAG, "JNI resume")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.resume()
        }
    }
    
    @JvmStatic
    fun stop() {
        Log.d(TAG, "JNI stop")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.stop()
        }
    }
    
    @JvmStatic
    fun setVolume(volume: Float) {
        Log.d(TAG, "JNI setVolume: $volume")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.setVolume(volume)
        }
    }
    
    @JvmStatic
    fun seekTo(positionMs: Long) {
        Log.d(TAG, "JNI seekTo: $positionMs")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.seekTo(positionMs)
        }
    }
    
    @JvmStatic
    fun setLoop(loop: Boolean) {
        Log.d(TAG, "JNI setLoop: $loop")
        runOnMainThread {
            AudioPlayerInstance.getInstance()?.setLoop(loop)
        }
    }
    
    @JvmStatic
    fun getPosition(): Long {
        return runOnMainThread {
            AudioPlayerInstance.getInstance()?.getPosition() ?: 0
        }
    }
    
    @JvmStatic
    fun getDuration(): Long {
        return runOnMainThread {
            AudioPlayerInstance.getInstance()?.getDuration() ?: 0
        }
    }
    
    @JvmStatic
    fun getCurrentUrl(): String {
        return runOnMainThread {
            AudioPlayerInstance.getInstance()?.getCurrentUrl() ?: ""
        }
    }
    
    @JvmStatic
    fun isPlaying(): Boolean {
        return runOnMainThread {
            val state = AudioPlayerInstance.getInstance()?.getState()
            state?.isPlaying ?: false
        }
    }
    
    /**
     * 在主线程执行代码，如果是主线程直接执行，否则 post 到主线程
     * 使用 CountDownLatch 等待结果返回
     */
    private fun <T> runOnMainThread(action: () -> T): T {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            // 已经在主线程，直接执行
            return action()
        }
        
        // 不在主线程，需要 post 到主线程并等待结果
        val latch = CountDownLatch(1)
        val result = arrayOfNulls<Any>(1)
        
        mainHandler.post {
            try {
                result[0] = action()
            } catch (e: Exception) {
                Log.e(TAG, "Error in main thread execution", e)
                result[0] = null
            } finally {
                latch.countDown()
            }
        }
        
        // 等待主线程执行完成（最多等 5 秒）
        latch.await(5, java.util.concurrent.TimeUnit.SECONDS)
        
        @Suppress("UNCHECKED_CAST")
        return result[0] as T
    }
}
