package com.pwa.container

import android.webkit.CookieManager
import android.webkit.WebView
import android.util.Log

object CookieHelper {
    private const val TAG = "CookieHelper"

    /**
     * 获取 WebView 的所有 Cookies（包括 HttpOnly）
     * @param url 目标 URL，如果为空则返回所有 cookies
     * @return cookies 字符串，格式为 "name1=value1; name2=value2"
     */
    @JvmStatic
    fun getCookies(url: String? = null): String {
        val cookieManager = CookieManager.getInstance()
        return if (url != null) {
            cookieManager.getCookie(url) ?: ""
        } else {
            // 获取所有已知的 cookies
            // 注意：Android CookieManager 没有直接获取所有 cookies 的方法
            // 需要维护一个已访问 URL 列表来获取所有 cookies
            cookieManager.getCookie(url) ?: ""
        }
    }

    /**
     * 获取指定域名的所有 Cookies
     * @param domain 域名，如 ".example.com"
     * @return cookies 字符串
     */
    @JvmStatic
    fun getCookiesForDomain(domain: String): String {
        val cookieManager = CookieManager.getInstance()
        // 构建一个该域名的 URL 来获取 cookies
        val url = "https://$domain"
        return cookieManager.getCookie(url) ?: ""
    }

    /**
     * 获取所有 WebView 实例的 Cookies
     * 这会尝试从当前活动的 WebView 获取
     */
    @JvmStatic
    fun getAllCookies(webView: WebView?): String {
        return try {
            val cookieManager = CookieManager.getInstance()
            val url = webView?.url
            if (url != null) {
                val cookies = cookieManager.getCookie(url)
                Log.d(TAG, "Got cookies for $url: ${cookies?.take(100)}...")
                cookies ?: ""
            } else {
                Log.w(TAG, "WebView URL is null")
                ""
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get cookies: ${e.message}", e)
            ""
        }
    }

    /**
     * 将 cookies 同步到 WebView
     * @param url 目标 URL
     * @param cookies cookies 字符串
     */
    @JvmStatic
    fun setCookies(url: String, cookies: String) {
        try {
            val cookieManager = CookieManager.getInstance()
            cookieManager.setCookie(url, cookies)
            cookieManager.flush()
            Log.d(TAG, "Set cookies for $url")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to set cookies: ${e.message}", e)
        }
    }
}
