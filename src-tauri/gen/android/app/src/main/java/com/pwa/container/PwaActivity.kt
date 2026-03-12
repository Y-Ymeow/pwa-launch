package com.pwa.container

import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding

class PwaActivity : TauriActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        
        // 刘海屏适配
        setupEdgeToEdgeDisplay()

        // 获取启动参数
        val appId = intent.getStringExtra("appId") ?: ""
        val appName = intent.getStringExtra("appName") ?: "PWA"
        val url = intent.getStringExtra("url") ?: ""

        if (url.isNotEmpty()) {
            // 设置标题
            title = appName

            // 延迟加载 URL
            android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                val webView = findViewById<WebView>(resources.getIdentifier("tauri_webview", "id", packageName))
                webView?.let {
                    it.webViewClient = WebViewClient()
                    it.loadUrl(url)
                }
            }, 100)
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        // 处理新的 Intent
        val url = intent.getStringExtra("url")
        if (!url.isNullOrEmpty()) {
            val webView = findViewById<WebView>(resources.getIdentifier("tauri_webview", "id", packageName))
            webView?.loadUrl(url)
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
}
