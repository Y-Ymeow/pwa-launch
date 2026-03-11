package com.pwa.container

import android.os.Bundle
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.enableEdgeToEdge

class PwaActivity : TauriActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

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
}
