package com.pwa.container

import android.view.View
import android.webkit.WebChromeClient
import android.webkit.WebView

/**
 * 支持视频全屏的 WebChromeClient
 * 继承 RustWebChromeClient 保留 Tauri 功能，同时添加视频全屏支持
 */
class FullscreenChromeClient(
    activity: WryActivity,
    private val mainActivity: MainActivity
) : RustWebChromeClient(activity) {

    override fun onShowCustomView(view: View, callback: CustomViewCallback) {
        // 调用 MainActivity 的全屏方法
        mainActivity.enterVideoFullscreen(view, callback)
    }

    override fun onHideCustomView() {
        // 调用 MainActivity 的退出全屏方法
        mainActivity.exitVideoFullscreen()
    }

    override fun onShowCustomView(
        view: View,
        requestedOrientation: Int,
        callback: CustomViewCallback
    ) {
        // Android 14+ 的新方法
        onShowCustomView(view, callback)
    }
}
