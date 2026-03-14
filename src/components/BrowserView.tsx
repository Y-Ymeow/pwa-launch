import { useRef, useEffect, useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BrowserHistoryItem } from "./types";

interface BrowserViewProps {
  browserUrl: string;
  setBrowserUrl: (url: string) => void;
  browserHistory: BrowserHistoryItem[];
  setBrowserHistory: (
    history:
      | BrowserHistoryItem[]
      | ((prev: BrowserHistoryItem[]) => BrowserHistoryItem[]),
  ) => void;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

export function BrowserView({
  browserUrl,
  setBrowserUrl,
  browserHistory,
  setBrowserHistory,
  onClose,
  showMessage,
}: BrowserViewProps) {
  const [inputUrl, setInputUrl] = useState(browserUrl);
  const [isNavigating, setIsNavigating] = useState(false);

  // 导航到 URL - 使用 navigate_to_url 直接跳转
  const navigateToUrl = useCallback(
    async (url: string) => {
      let finalUrl = url.trim();
      if (!finalUrl) return;

      if (!finalUrl.startsWith("http://") && !finalUrl.startsWith("https://")) {
        finalUrl = "https://" + finalUrl;
      }

      setBrowserUrl(finalUrl);
      setInputUrl(finalUrl);
      setIsNavigating(true);

      try {
        // 使用 Tauri 命令直接在当前 WebView 跳转
        await invoke("navigate_to_url", { url: finalUrl });

        // 添加到历史记录
        setBrowserHistory((prev) =>
          [
            { url: finalUrl, title: finalUrl, timestamp: Date.now() },
            ...prev.filter((h) => h.url !== finalUrl),
          ].slice(0, 50),
        );

        showMessage("success", "正在打开...");
      } catch (error) {
        showMessage("error", `打开失败: ${String(error)}`);
      } finally {
        setIsNavigating(false);
      }
    },
    [setBrowserUrl, setBrowserHistory, showMessage],
  );

  // 初始加载
  useEffect(() => {
    if (browserUrl && !isNavigating) {
      navigateToUrl(browserUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="browser-view" style={{ padding: "20px" }}>
      {/* 地址栏 */}
      <div
        className="browser-local-bar"
        style={{
          display: "flex",
          gap: "10px",
          marginBottom: "20px",
          alignItems: "center",
        }}
      >
        <button
          onClick={onClose}
          className="browser-btn"
          style={{ padding: "8px 16px" }}
        >
          ←
        </button>

        <form
          onSubmit={(e) => {
            e.preventDefault();
            navigateToUrl(inputUrl);
          }}
          style={{ flex: 1, display: "flex", gap: "8px" }}
        >
          <input
            type="text"
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            placeholder="输入网址..."
            className="browser-address-input"
            style={{
              flex: 1,
              padding: "8px 12px",
              borderRadius: "8px",
              border: "1px solid rgba(255,255,255,0.2)",
              background: "rgba(0,0,0,0.3)",
              color: "white",
            }}
          />
          <button
            type="submit"
            className="browser-go-btn"
            disabled={isNavigating}
            style={{
              padding: "8px 8px",
              background: "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)",
              border: "none",
              borderRadius: "8px",
              color: "white",
              cursor: isNavigating ? "not-allowed" : "pointer",
            }}
          >
            {isNavigating ? "..." : "GO"}
          </button>
        </form>
      </div>

      {/* 说明 */}
      <div
        style={{
          background: "rgba(255,255,255,0.1)",
          padding: "20px",
          borderRadius: "12px",
          marginBottom: "20px",
        }}
      >
        <h3 style={{ marginBottom: "10px" }}>浏览器模式</h3>
        <p>输入网址后将直接在当前窗口打开网站。</p>
        <p style={{ marginTop: "10px", fontSize: "14px", opacity: 0.8 }}>
          💡 提示：此模式 100% 兼容所有网站，包括需要人机验证的网站。
          但无法后台播放，返回应用列表可回到 PWA 容器。
        </p>
      </div>

      {/* 同步 Cookies 按钮 */}
      <div style={{ marginBottom: "20px" }}>
        <button
          onClick={async () => {
            try {
              const domain = new URL(browserUrl).hostname;
              await invoke("sync_webview_cookies", {
                domain,
                cookies: document.cookie,
                userAgent: navigator.userAgent,
              });
              showMessage("success", `已同步 ${domain} 的 Cookies`);
            } catch (error) {
              showMessage("error", `同步失败: ${String(error)}`);
            }
          }}
          style={{
            padding: "10px 20px",
            background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
            border: "none",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          🍪 同步当前页面 Cookies
        </button>
        <p style={{ fontSize: "12px", opacity: 0.6, marginTop: "8px" }}>
          点击此按钮将当前页面的登录状态同步到 PWA 模式
        </p>
      </div>

      {/* 历史记录 */}
      {browserHistory.length > 0 && (
        <div>
          <h4 style={{ marginBottom: "10px" }}>最近访问</h4>
          {browserHistory.slice(0, 10).map((item, idx) => (
            <div
              key={idx}
              onClick={() => navigateToUrl(item.url)}
              style={{
                padding: "10px",
                background: "rgba(255,255,255,0.05)",
                borderRadius: "8px",
                marginBottom: "8px",
                cursor: "pointer",
              }}
            >
              <div style={{ fontWeight: 500 }}>{item.title || item.url}</div>
              <div style={{ fontSize: "12px", opacity: 0.6 }}>{item.url}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
