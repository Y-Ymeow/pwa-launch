package com.pwa.container

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding

class MainActivity : TauriActivity() {

    private var pwaLaunchReceiver: BroadcastReceiver? = null
    private val handler = Handler(Looper.getMainLooper())
    
    // 视频全屏相关
    private var customView: View? = null
    private var customViewCallback: WebChromeClient.CustomViewCallback? = null
    private var originalOrientation: Int = 0
    private var originalSystemUiVisibility: Int = 0

    companion object {
        var pendingPwaUrl: String? = null
        var pendingPwaAppId: String? = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        
        // 启用 WebView 远程调试（允许 Chrome DevTools 连接）
        WebView.setWebContentsDebuggingEnabled(true)
        
        // 刘海屏适配：允许内容延伸到刘海区域
        setupEdgeToEdgeDisplay()
        
        // 延迟设置支持全屏的 WebChromeClient
        handler.postDelayed({
            setupFullscreenChromeClient()
        }, 500)

        // 注册广播接收器
        pwaLaunchReceiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) {
                if (intent?.action == "com.pwa.container.LAUNCH_PWA") {
                    val appId = intent.getStringExtra("appId")
                    val name = intent.getStringExtra("name")
                    val url = intent.getStringExtra("url")

                    if (url != null) {
                        // 存储 URL，等待 WebView 准备好
                        pendingPwaUrl = url
                        pendingPwaAppId = appId

                        // 尝试通过 Rust 事件通知前端
                        sendPwaLaunchEvent(appId, name, url)
                    }
                }
            }
        }

        registerReceiver(pwaLaunchReceiver, IntentFilter("com.pwa.container.LAUNCH_PWA"), RECEIVER_EXPORTED)

        // 处理启动 Intent
        handleIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        // 如果有待处理的 PWA URL，发送事件
        pendingPwaUrl?.let { url ->
            pendingPwaAppId?.let { appId ->
                sendPwaLaunchEvent(appId, null, url)
            }
        }
    }

    private fun sendPwaLaunchEvent(appId: String?, name: String?, url: String) {
        handler.postDelayed({
            // 使用 Tauri 的 event system 发送事件给前端
            val intent = Intent("tauri://event/launch-pwa")
            intent.putExtra("appId", appId)
            intent.putExtra("name", name)
            intent.putExtra("url", url)
            sendBroadcast(intent)
        }, 500)
    }

    override fun onDestroy() {
        super.onDestroy()
        pwaLaunchReceiver?.let { unregisterReceiver(it) }
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        // 如果处于视频全屏模式，先退出全屏
        if (isVideoFullscreen()) {
            exitVideoFullscreen()
            return
        }
        super.onBackPressed()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        val data = intent?.data
        if (data != null && data.scheme == "tauri-pwa" && data.host == "open") {
            val appId = data.getQueryParameter("appId")
            val url = data.getQueryParameter("url")

            if (url != null) {
                pendingPwaUrl = url
                pendingPwaAppId = appId

                handler.postDelayed({
                    val eventIntent = Intent("tauri://event/shortcut-open")
                    eventIntent.putExtra("appId", appId)
                    eventIntent.putExtra("url", url)
                    sendBroadcast(eventIntent)
                }, 500)
            }
        }
    }
    
    /**
     * 刘海屏/全面屏适配
     * 让内容延伸到刘海区域，并正确处理 WindowInsets
     */
    private fun setupEdgeToEdgeDisplay() {
        val window = window
        val decorView = window.decorView
        
        // 允许内容延伸到刘海区域（Android 9+）
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.P) {
            val params = window.attributes
            params.layoutInDisplayCutoutMode = 
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            window.attributes = params
        }
        
        // 设置全屏布局，让内容延伸到状态栏和导航栏下方
        WindowCompat.setDecorFitsSystemWindows(window, false)
        
        // 处理 WindowInsets，为系统栏预留空间
        ViewCompat.setOnApplyWindowInsetsListener(decorView) { view, windowInsets ->
            val insets = windowInsets.getInsets(WindowInsetsCompat.Type.systemBars())
            // 为根视图添加 padding，避免内容被状态栏/导航栏遮挡
            view.updatePadding(
                left = insets.left,
                top = insets.top,
                right = insets.right,
                bottom = insets.bottom
            )
            WindowInsetsCompat.CONSUMED
        }
    }
    
    /**
     * 视频全屏支持 - 进入全屏
     */
    fun enterVideoFullscreen(view: View, callback: WebChromeClient.CustomViewCallback) {
        if (customView != null) {
            callback.onCustomViewHidden()
            return
        }
        
        // 保存原始状态
        customView = view
        customViewCallback = callback
        originalOrientation = requestedOrientation
        originalSystemUiVisibility = window.decorView.systemUiVisibility
        
        // 隐藏状态栏和导航栏，进入沉浸式全屏
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_FULLSCREEN
            or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
            or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
            or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
            or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        )
        
        // 添加到内容视图
        val decor = window.decorView as FrameLayout
        decor.addView(view, FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ))
        
        // 强制横屏（可选，根据需要）
        // requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
    }
    
    /**
     * 视频全屏支持 - 退出全屏
     */
    fun exitVideoFullscreen() {
        if (customView == null) return
        
        // 恢复系统 UI
        window.decorView.systemUiVisibility = originalSystemUiVisibility
        
        // 从视图中移除
        val decor = window.decorView as FrameLayout
        decor.removeView(customView)
        
        // 恢复原始方向
        // requestedOrientation = originalOrientation
        
        // 通知 WebView
        customViewCallback?.onCustomViewHidden()
        
        customView = null
        customViewCallback = null
    }
    
    /**
     * 检查当前是否处于视频全屏模式
     */
    fun isVideoFullscreen(): Boolean = customView != null
    
    /**
     * 设置支持视频全屏的 WebChromeClient
     */
    private fun setupFullscreenChromeClient() {
        try {
            val webView = findViewById<WebView>(resources.getIdentifier("tauri_webview", "id", packageName))
            webView?.let {
                val chromeClient = FullscreenChromeClient(this as WryActivity, this)
                it.webChromeClient = chromeClient
                android.util.Log.d("MainActivity", "FullscreenChromeClient 设置成功")
            }
        } catch (e: Exception) {
            android.util.Log.e("MainActivity", "设置 FullscreenChromeClient 失败: ${e.message}")
        }
    }
}
