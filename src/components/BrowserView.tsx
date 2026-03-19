import { useRef, useEffect, useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir } from "@tauri-apps/api/path";
import type { BrowserHistoryItem, BrowserBookmarkItem } from "./types";
import { clearCookies, getCookieDomains, syncWebviewCookies } from "../cookie";

interface BrowserViewProps {
  browserUrl: string;
  setBrowserUrl: (url: string) => void;
  browserHistory: BrowserHistoryItem[];
  setBrowserHistory: (
    history:
      | BrowserHistoryItem[]
      | ((prev: BrowserHistoryItem[]) => BrowserHistoryItem[]),
  ) => void;
  browserBookmarks: BrowserBookmarkItem[];
  setBrowserBookmarks: (
    bookmarks:
      | BrowserBookmarkItem[]
      | ((prev: BrowserBookmarkItem[]) => BrowserBookmarkItem[]),
  ) => void;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

export function BrowserView({
  browserUrl,
  setBrowserUrl,
  browserHistory,
  setBrowserHistory,
  browserBookmarks,
  setBrowserBookmarks,
  onClose,
  showMessage,
}: BrowserViewProps) {
  const [inputUrl, setInputUrl] = useState(browserUrl && browserUrl !== "about:blank" ? browserUrl : "");
  const [isNavigating, setIsNavigating] = useState(false);
  const [showBookmarks, setShowBookmarks] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [isInputFocused, setIsInputFocused] = useState(false);
  const [showCookieManager, setShowCookieManager] = useState(false);
  const [cookieDomains, setCookieDomains] = useState<string[]>([]);
  const [isLoadingDomains, setIsLoadingDomains] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);

  // 只在没有输入焦点时同步 URL，并过滤 about:blank
  useEffect(() => {
    if (!isInputFocused && browserUrl && browserUrl !== "about:blank") {
      setInputUrl(browserUrl);
    }
  }, [browserUrl, isInputFocused]);

  // 智能 URL 处理 - 自动添加协议
  const processUrl = (url: string): string => {
    let finalUrl = url.trim();
    if (!finalUrl) return "";

    if (finalUrl.startsWith("http://") || finalUrl.startsWith("https://")) {
      return finalUrl;
    }

    if (
      finalUrl.startsWith("localhost") ||
      finalUrl.startsWith("127.") ||
      /^\d+\.\d+\.\d+\.\d+/.test(finalUrl)
    ) {
      return "http://" + finalUrl;
    }

    return "https://" + finalUrl;
  };

  // 导航到 URL
  const navigateToUrl = useCallback(
    (url: string) => {
      const finalUrl = processUrl(url);
      if (!finalUrl) return;

      setBrowserUrl(finalUrl);
      setInputUrl(finalUrl);
      setIsNavigating(true);

      // 直接在当前窗口打开
      window.location.href = finalUrl;

      // 添加到历史记录
      setBrowserHistory((prev) =>
        [
          { url: finalUrl, title: finalUrl, timestamp: Date.now() },
          ...prev.filter((h) => h.url !== finalUrl),
        ].slice(0, 50),
      );

      showMessage("success", "正在打开...");
      setIsNavigating(false);
    },
    [setBrowserUrl, setBrowserHistory, showMessage],
  );

  // 添加收藏
  const addBookmark = useCallback(() => {
    const currentUrl = browserUrl && browserUrl !== "about:blank" ? browserUrl : inputUrl;
    if (!currentUrl) {
      showMessage("error", "没有可收藏的页面");
      return;
    }

    const exists = browserBookmarks.find((b) => b.url === currentUrl);
    if (exists) {
      showMessage("success", "已收藏");
      return;
    }

    const newBookmark: BrowserBookmarkItem = {
      url: currentUrl,
      title: currentUrl,
      createdAt: Date.now(),
    };

    setBrowserBookmarks((prev) => [newBookmark, ...prev]);
    showMessage("success", "已添加到收藏");
  }, [browserUrl, inputUrl, browserBookmarks, setBrowserBookmarks, showMessage]);

  // 删除收藏
  const removeBookmark = useCallback((url: string) => {
    setBrowserBookmarks((prev) => prev.filter((b) => b.url !== url));
  }, [setBrowserBookmarks]);

  // 清空历史
  const clearHistory = useCallback(() => {
    setBrowserHistory([]);
    showMessage("success", "历史记录已清空");
  }, [setBrowserHistory, showMessage]);

  // 加载所有 Cookie 域名
  const loadCookieDomains = useCallback(async () => {
    setIsLoadingDomains(true);
    try {
      const domains = await getCookieDomains();
      setCookieDomains(domains);
    } catch (error) {
      showMessage("error", `加载域名失败: ${String(error)}`);
    } finally {
      setIsLoadingDomains(false);
    }
  }, [showMessage]);

  // 删除指定域名的 Cookies
  const deleteDomainCookies = useCallback(async (domain: string) => {
    try {
      await clearCookies("browser", domain, true);
      await clearCookies("webview", domain, true);
      showMessage("success", `已删除 ${domain} 及其子域的 Cookies`);
      // 刷新列表
      loadCookieDomains();
    } catch (error) {
      showMessage("error", `删除失败: ${String(error)}`);
    }
  }, [loadCookieDomains, showMessage]);

  // 初始加载
  useEffect(() => {
    if (browserUrl && browserUrl !== "about:blank" && !isNavigating) {
      navigateToUrl(browserUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 打开 Cookie 管理器时加载域名
  useEffect(() => {
    if (showCookieManager) {
      loadCookieDomains();
    }
  }, [showCookieManager, loadCookieDomains]);

  return (
    <div className="browser-view" style={{ 
      padding: "20px",
      display: "flex",
      flexDirection: "column",
      height: "100vh",
      overflow: "hidden"
    }}>
      {/* 工具栏 - 固定在顶部 */}
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
          style={{
            padding: "8px 16px",
            background: "rgba(255,255,255,0.1)",
            border: "1px solid rgba(255,255,255,0.2)",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "16px",
          }}
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
            ref={inputRef}
            type="text"
            inputMode="url"
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck="false"
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            onFocus={() => setIsInputFocused(true)}
            onBlur={() => setIsInputFocused(false)}
            placeholder="输入网址..."
            className="browser-address-input"
            style={{
              flex: 1,
              padding: "10px 12px",
              borderRadius: "8px",
              border: "1px solid rgba(255,255,255,0.2)",
              background: "rgba(0,0,0,0.3)",
              color: "white",
              fontSize: "16px",
              outline: "none",
              WebkitAppearance: "none",
              minWidth: 0,
            }}
          />
          <button
            type="submit"
            className="browser-go-btn"
            disabled={isNavigating}
            style={{
              padding: "10px 16px",
              background: "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)",
              border: "none",
              borderRadius: "8px",
              color: "white",
              cursor: isNavigating ? "not-allowed" : "pointer",
              fontSize: "14px",
              fontWeight: "bold",
              whiteSpace: "nowrap",
            }}
          >
            {isNavigating ? "..." : "GO"}
          </button>
        </form>
      </div>

      {/* 快捷操作按钮 */}
      <div
        style={{
          display: "flex",
          gap: "10px",
          marginBottom: "20px",
          flexWrap: "wrap",
        }}
      >
        <button
          onClick={addBookmark}
          style={{
            padding: "8px 16px",
            background: "rgba(255,255,255,0.1)",
            border: "1px solid rgba(255,255,255,0.2)",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          ⭐ 收藏当前页
        </button>

        <button
          onClick={() => {
            setShowBookmarks(!showBookmarks);
            setShowHistory(false);
          }}
          style={{
            padding: "8px 16px",
            background: showBookmarks
              ? "rgba(102,126,234,0.5)"
              : "rgba(255,255,255,0.1)",
            border: "1px solid rgba(255,255,255,0.2)",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          📑 收藏夹 ({browserBookmarks.length})
        </button>

        <button
          onClick={() => {
            setShowHistory(!showHistory);
            setShowBookmarks(false);
          }}
          style={{
            padding: "8px 16px",
            background: showHistory
              ? "rgba(102,126,234,0.5)"
              : "rgba(255,255,255,0.1)",
            border: "1px solid rgba(255,255,255,0.2)",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          🕐 历史 ({browserHistory.length})
        </button>

        <button
          onClick={() => {
            setShowCookieManager(true);
            setShowBookmarks(false);
            setShowHistory(false);
          }}
          style={{
            padding: "8px 16px",
            background: showCookieManager
              ? "rgba(245,87,108,0.5)"
              : "rgba(255,255,255,0.1)",
            border: "1px solid rgba(255,255,255,0.2)",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          🍪 管理 Cookies
        </button>
      </div>

      {/* 收藏夹面板 */}
      {showBookmarks && (
        <div
          style={{
            background: "rgba(0,0,0,0.3)",
            borderRadius: "12px",
            padding: "16px",
            marginBottom: "20px",
            maxHeight: "300px",
            overflowY: "auto",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "12px",
            }}
          >
            <h4 style={{ margin: 0, color: "white" }}>📑 收藏夹 ({browserBookmarks.length})</h4>
            <button
              onClick={() => setShowBookmarks(false)}
              style={{
                background: "none",
                border: "none",
                color: "rgba(255,255,255,0.6)",
                cursor: "pointer",
                fontSize: "18px",
              }}
            >
              ✕
            </button>
          </div>

          {browserBookmarks.length === 0 ? (
            <div
              style={{
                textAlign: "center",
                color: "rgba(255,255,255,0.5)",
                padding: "20px",
              }}
            >
              暂无收藏
            </div>
          ) : (
            browserBookmarks.map((item, idx) => (
              <div
                key={idx}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "10px",
                  padding: "10px",
                  background: "rgba(255,255,255,0.05)",
                  borderRadius: "8px",
                  marginBottom: "8px",
                }}
              >
                <div
                  style={{ flex: 1, cursor: "pointer", minWidth: 0 }}
                  onClick={() => {
                    navigateToUrl(item.url);
                    setShowBookmarks(false);
                  }}
                >
                  <div style={{ fontWeight: 500, marginBottom: "4px", color: "white", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {item.title || item.url}
                  </div>
                  <div
                    style={{ fontSize: "12px", opacity: 0.6, wordBreak: "break-all", color: "rgba(255,255,255,0.7)" }}
                  >
                    {item.url}
                  </div>
                </div>
                <button
                  onClick={() => removeBookmark(item.url)}
                  style={{
                    background: "none",
                    border: "none",
                    color: "#f5576c",
                    cursor: "pointer",
                    fontSize: "16px",
                    padding: "4px",
                    flexShrink: 0,
                  }}
                  title="删除"
                >
                  🗑️
                </button>
              </div>
            ))
          )}
        </div>
      )}

      {/* 历史记录面板 */}
      {showHistory && (
        <div
          style={{
            background: "rgba(0,0,0,0.3)",
            borderRadius: "12px",
            padding: "16px",
            marginBottom: "20px",
            maxHeight: "300px",
            overflowY: "auto",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "12px",
            }}
          >
            <h4 style={{ margin: 0, color: "white" }}>🕐 历史记录 ({browserHistory.length})</h4>
            <div style={{ display: "flex", gap: "8px" }}>
              <button
                onClick={clearHistory}
                style={{
                  background: "rgba(239,68,68,0.2)",
                  border: "none",
                  color: "#f5576c",
                  padding: "4px 12px",
                  borderRadius: "4px",
                  cursor: "pointer",
                  fontSize: "12px",
                }}
              >
                清空
              </button>
              <button
                onClick={() => setShowHistory(false)}
                style={{
                  background: "none",
                  border: "none",
                  color: "rgba(255,255,255,0.6)",
                  cursor: "pointer",
                  fontSize: "18px",
                }}
              >
                ✕
              </button>
            </div>
          </div>

          {browserHistory.length === 0 ? (
            <div
              style={{
                textAlign: "center",
                color: "rgba(255,255,255,0.5)",
                padding: "20px",
              }}
            >
              暂无历史记录
            </div>
          ) : (
            browserHistory.slice(0, 30).map((item, idx) => (
              <div
                key={idx}
                onClick={() => {
                  navigateToUrl(item.url);
                  setShowHistory(false);
                }}
                style={{
                  padding: "10px",
                  background: "rgba(255,255,255,0.05)",
                  borderRadius: "8px",
                  marginBottom: "8px",
                  cursor: "pointer",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontWeight: 500,
                      marginBottom: "4px",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      color: "white",
                    }}
                  >
                    {item.title || item.url}
                  </div>
                  <div
                    style={{
                      fontSize: "12px",
                      opacity: 0.6,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      color: "rgba(255,255,255,0.7)",
                    }}
                  >
                    {item.url}
                  </div>
                </div>
                <div
                  style={{
                    fontSize: "11px",
                    opacity: 0.4,
                    marginLeft: "8px",
                    flexShrink: 0,
                    color: "rgba(255,255,255,0.5)",
                  }}
                >
                  {new Date(item.timestamp).toLocaleDateString()}
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 说明 */}
      <div
        style={{
          flex: 1,
          overflowY: "auto",
          background: "rgba(255,255,255,0.1)",
          padding: "20px",
          borderRadius: "12px",
          marginBottom: "20px",
          color: "white",
        }}
      >
        <h3 style={{ margin: 0, marginBottom: "10px" }}>浏览器模式</h3>
        <p>输入网址后将直接在当前窗口打开网站。</p>
        <p style={{ marginTop: "10px", fontSize: "14px", opacity: 0.8 }}>
          💡 提示：此模式 100% 兼容所有网站，包括需要人机验证的网站。
          但无法后台播放，返回应用列表可回到 PWA 容器。
        </p>
      </div>

      {/* Cookies 操作按钮 */}
      <div style={{ marginBottom: "20px", display: "flex", gap: "10px", flexWrap: "wrap" }}>
        <button
          onClick={async () => {
            try {
              const domain = new URL(browserUrl).hostname;
              await syncWebviewCookies(domain, document.cookie, navigator.userAgent);
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
          🍪 同步 Cookies
        </button>

        <button
          onClick={async () => {
            try {
              if (!browserUrl || browserUrl === "about:blank") {
                showMessage("error", "当前没有打开的页面");
                return;
              }
              const domain = new URL(browserUrl).hostname;
              await clearCookies("browser", domain, true);
              await clearCookies("webview", domain, true);
              showMessage("success", `已清除 ${domain} 及其子域的 Cookies`);
            } catch (error) {
              showMessage("error", `清除失败: ${String(error)}`);
            }
          }}
          style={{
            padding: "10px 20px",
            background: "linear-gradient(135deg, #f5576c 0%, #f093fb 100%)",
            border: "none",
            borderRadius: "8px",
            color: "white",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          🗑️ 清除 Cookies
        </button>
      </div>
      <p style={{ fontSize: "12px", opacity: 0.6, marginTop: "-15px", marginBottom: "20px", color: "rgba(255,255,255,0.7)" }}>
        同步：保存登录状态到数据库 | 清除：删除当前网站的所有 Cookies（包括子域）
      </p>

      {/* Cookie 管理弹窗 */}
      {showCookieManager && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(0,0,0,0.7)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000,
          }}
          onClick={() => setShowCookieManager(false)}
        >
          <div
            style={{
              background: "#1a1a2e",
              borderRadius: "16px",
              padding: "24px",
              width: "90%",
              maxWidth: "500px",
              maxHeight: "80vh",
              overflowY: "auto",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                marginBottom: "20px",
              }}
            >
              <h3 style={{ margin: 0, color: "white" }}>🍪 Cookie 管理</h3>
              <button
                onClick={() => setShowCookieManager(false)}
                style={{
                  background: "none",
                  border: "none",
                  color: "rgba(255,255,255,0.6)",
                  cursor: "pointer",
                  fontSize: "24px",
                }}
              >
                ✕
              </button>
            </div>

            <p style={{ color: "rgba(255,255,255,0.7)", fontSize: "14px", marginBottom: "16px" }}>
              以下是所有存储了 Cookies 的域名，点击删除可清除该域名及其子域的所有 Cookies
            </p>

            {isLoadingDomains ? (
              <div style={{ textAlign: "center", padding: "40px", color: "rgba(255,255,255,0.5)" }}>
                加载中...
              </div>
            ) : cookieDomains.length === 0 ? (
              <div style={{ textAlign: "center", padding: "40px", color: "rgba(255,255,255,0.5)" }}>
                暂无存储的 Cookies
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                {cookieDomains.map((domain) => (
                  <div
                    key={domain}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      padding: "12px 16px",
                      background: "rgba(255,255,255,0.05)",
                      borderRadius: "8px",
                    }}
                  >
                    <span style={{ color: "white", fontSize: "14px" }}>{domain}</span>
                    <button
                      onClick={() => deleteDomainCookies(domain)}
                      style={{
                        padding: "6px 12px",
                        background: "rgba(245,87,108,0.2)",
                        border: "1px solid rgba(245,87,108,0.5)",
                        borderRadius: "6px",
                        color: "#f5576c",
                        cursor: "pointer",
                        fontSize: "12px",
                      }}
                    >
                      删除
                    </button>
                  </div>
                ))}
              </div>
            )}

            <div style={{ marginTop: "20px", display: "flex", gap: "10px" }}>
              <button
                onClick={loadCookieDomains}
                style={{
                  flex: 1,
                  padding: "10px",
                  background: "rgba(102,126,234,0.2)",
                  border: "1px solid rgba(102,126,234,0.5)",
                  borderRadius: "8px",
                  color: "#667eea",
                  cursor: "pointer",
                  fontSize: "14px",
                }}
              >
                🔄 刷新列表
              </button>
              <button
                onClick={() => setShowCookieManager(false)}
                style={{
                  flex: 1,
                  padding: "10px",
                  background: "rgba(255,255,255,0.1)",
                  border: "1px solid rgba(255,255,255,0.2)",
                  borderRadius: "8px",
                  color: "white",
                  cursor: "pointer",
                  fontSize: "14px",
                }}
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
