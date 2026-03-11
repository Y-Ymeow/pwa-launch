package com.pwa.container

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {

    private var pwaLaunchReceiver: BroadcastReceiver? = null
    private val handler = Handler(Looper.getMainLooper())

    companion object {
        var pendingPwaUrl: String? = null
        var pendingPwaAppId: String? = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

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
}
