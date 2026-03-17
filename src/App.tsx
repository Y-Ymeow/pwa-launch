import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./styles/App.css";
import {
  BrowserView,
  AppList,
  ProxySettings,
  AppSettings,
  type AppInfo,
  type RunningPwa,
  type PwaSnapshot,
  type CommandResponse,
  type ViewMode,
  type BrowserHistoryItem,
  type BrowserBookmarkItem,
} from "./components";
import Test from "./Test.js";

const MAX_IFRAMES = 6;

// KV 存储键
const KV_APP_ID = "browser";
const KV_KEY_HISTORY = "history";
const KV_KEY_BOOKMARKS = "bookmarks";

function App() {
  // 视图模式
  const [viewMode, setViewMode] = useState<ViewMode>("apps");

  // 浏览器状态
  const [browserUrl, setBrowserUrl] = useState("");
  const [browserHistory, setBrowserHistory] = useState<BrowserHistoryItem[]>(
    [],
  );
  const [browserBookmarks, setBrowserBookmarks] = useState<
    BrowserBookmarkItem[]
  >([]);
  const [browserDataLoaded, setBrowserDataLoaded] = useState(false);

  // 从 KV 加载浏览器数据
  useEffect(() => {
    const loadBrowserData = async () => {
      try {
        const historyRes = await invoke<CommandResponse<string | null>>(
          "kv_get",
          {
            appId: KV_APP_ID,
            key: KV_KEY_HISTORY,
          },
        );
        if (historyRes.success && historyRes.data) {
          setBrowserHistory(JSON.parse(historyRes.data));
        }

        const bookmarksRes = await invoke<CommandResponse<string | null>>(
          "kv_get",
          {
            appId: KV_APP_ID,
            key: KV_KEY_BOOKMARKS,
          },
        );
        if (bookmarksRes.success && bookmarksRes.data) {
          setBrowserBookmarks(JSON.parse(bookmarksRes.data));
        }
      } catch (e) {
        console.error("Failed to load browser data from KV:", e);
      } finally {
        setBrowserDataLoaded(true);
      }
    };
    loadBrowserData();
  }, []);

  // 保存历史到 KV
  useEffect(() => {
    if (!browserDataLoaded) return;
    const saveHistory = async () => {
      try {
        await invoke("kv_set", {
          appId: KV_APP_ID,
          key: KV_KEY_HISTORY,
          value: JSON.stringify(browserHistory),
        });
      } catch (e) {
        console.error("Failed to save history to KV:", e);
      }
    };
    saveHistory();
  }, [browserHistory, browserDataLoaded]);

  // 保存收藏到 KV
  useEffect(() => {
    if (!browserDataLoaded) return;
    const saveBookmarks = async () => {
      try {
        await invoke("kv_set", {
          appId: KV_APP_ID,
          key: KV_KEY_BOOKMARKS,
          value: JSON.stringify(browserBookmarks),
        });
      } catch (e) {
        console.error("Failed to save bookmarks to KV:", e);
      }
    };
    saveBookmarks();
  }, [browserBookmarks, browserDataLoaded]);

  // 应用状态
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [runningPwas, setRunningPwas] = useState<RunningPwa[]>([]);
  const [activePwaId, setActivePwaId] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState<Record<string, PwaSnapshot>>({});
  const [restoringPwa, setRestoringPwa] = useState<string | null>(null);

  // 代理设置
  const [showProxy, setShowProxy] = useState(false);

  // 应用设置
  const [showAppSettings, setShowAppSettings] = useState(false);

  // 消息提示
  const [message, setMessage] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // 悬浮面板显示状态
  const [showSwitcherPanel, setShowSwitcherPanel] = useState(false);

  const iframesRef = useRef<Record<string, HTMLIFrameElement>>({});

  // 加载应用列表
  const loadApps = async () => {
    try {
      const response = await invoke<CommandResponse<AppInfo[]>>("list_apps");
      if (response.success && response.data) {
        setApps(response.data);
      }
    } catch (error) {
      showMessage("error", `加载应用列表失败：${error}`);
    }
  };

  // 显示消息
  const showMessage = (type: "success" | "error", text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  };

  // 初始化
  useEffect(() => {
    loadApps();

    // 加载屏幕常亮设置
    const loadKeepScreenOn = async () => {
      try {
        const result = await invoke<{
          success: boolean;
          data: boolean | string | null;
        }>("get_app_config", { key: "keep_screen_on" });
        if (result.success && result.data) {
          // 处理字符串或布尔值
          const val = result.data;
          const enabled = typeof val === "boolean" ? val : val === "true";
          if (enabled) {
            await invoke("set_keep_screen_on", { enabled: true });
          }
        }
      } catch (e) {
        console.error("Failed to load keep screen on setting:", e);
      }
    };
    loadKeepScreenOn();

    // 监听 iframe 消息
    const handleMessage = async (event: MessageEvent) => {
      if (event.data?.type === "ADAPT_READY") {
        event.source?.postMessage({ type: "ADAPT_PARENT_READY" }, "*");
        return;
      }

      // 处理浏览器模式的 Cookies 同步
      if (event.data?.type === "BROWSER_SYNC_COOKIES") {
        const { domain, cookies } = event.data;
        if (domain && cookies !== undefined) {
          try {
            await invoke("sync_webview_cookies", {
              domain,
              cookies,
              userAgent: navigator.userAgent,
            });
            showMessage("success", `已同步 ${domain} 的 Cookies`);
          } catch (error) {
            showMessage("error", `同步 Cookies 失败: ${String(error)}`);
          }
        }
        return;
      }

      // 处理 HTTP 代理请求（通过本地服务器，支持并发和流式传输）
      if (event.data?.type === "ADAPT_PROXY_REQUEST") {
        const { requestId, url, method, headers, body, isMedia, isXHR } =
          event.data;
        let requestBody = body;
        console.log("[App] Proxy request:", {
          requestId,
          url,
          method,
          isMedia,
          hasHeaders: !!headers,
          hasBody: !!body,
          bodyType: typeof body,
          bodyValue: requestBody,
        });
        try {
          // 把所有请求参数放在 body 里传给代理服务器
          // 如果url是本地服务
          if (url.startsWith("http://localhost:19315")) {
            return await fetch(url);
          }
          // 父窗口的 fetch 只设置 Content-Type，不设置自定义 headers（避免浏览器拦截）
          const proxyBodyObj = {
            target: url,
            method: method || "GET",
            headers: headers || {},
            body: requestBody || null,
            isXHR: isXHR || false,
          };
          console.log("[App] Proxy body:", proxyBodyObj);

          const proxyBody = JSON.stringify(proxyBodyObj);

          // 根据 isMedia 选择路由：媒体请求走 /media/proxy（禁用 gzip），普通请求走 /api/proxy
          const proxyUrl = isMedia
            ? "http://localhost:19315/media/proxy"
            : "http://localhost:19315/api/proxy";
          console.log("[App] Proxy URL:", proxyUrl);

          const proxyResponse = await fetch(proxyUrl, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              // 不在这里添加其他 headers，防止浏览器拦截
            },
            body: proxyBody,
          });

          // 检测是否为二进制响应（图片、音频、视频）
          const contentType = proxyResponse.headers.get("content-type") || "";
          const isBinary =
            contentType.startsWith("image/") ||
            contentType.startsWith("audio/") ||
            contentType.startsWith("video/") ||
            contentType === "application/octet-stream";

          let responseBody: string;
          if (isBinary) {
            // 二进制数据转为 base64
            const arrayBuffer = await proxyResponse.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);
            let binary = "";
            for (let i = 0; i < bytes.byteLength; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            responseBody = btoa(binary);
          } else {
            // 文本数据直接使用
            responseBody = await proxyResponse.text();
          }

          const responseData = {
            status: proxyResponse.status,
            statusText: proxyResponse.statusText,
            headers: Object.fromEntries(proxyResponse.headers.entries()),
            body: responseBody,
            isBase64: isBinary,
          };

          event.source?.postMessage(
            {
              type: "ADAPT_PROXY_RESPONSE",
              requestId,
              success: true,
              data: responseData,
            },
            "*",
          );
        } catch (error) {
          console.error("[App] Proxy request failed:", error);
          event.source?.postMessage(
            {
              type: "ADAPT_PROXY_RESPONSE",
              requestId,
              success: false,
              error: String(error),
            },
            "*",
          );
        }
        return;
      }

      const iframe = Object.values(iframesRef.current).find(
        (f) => f.contentWindow === event.source,
      );
      if (!iframe) return;

      if (event.data?.type === "ADAPT_INVOKE") {
        const { cmd, payload, requestId } = event.data;
        try {
          // 找到对应的 appId
          const entry = Object.entries(iframesRef.current).find(
            ([_, f]) => f.contentWindow === event.source,
          );
          const appId = entry ? entry[0] : null;

          // 为 SQLite 和 KV 命令自动注入 appId/pwaId
          let finalPayload = payload;
          if (appId) {
            if (cmd.startsWith("sqlite_")) {
              finalPayload = { ...payload, pwaId: appId };
            } else if (cmd.startsWith("kv_")) {
              finalPayload = { ...payload, appId: appId };
            }
          }

          const result = await invoke(cmd, finalPayload);
          event.source?.postMessage(
            {
              type: "ADAPT_RESULT",
              cmd,
              requestId, // 返回 requestId 以便匹配并发请求
              result: JSON.parse(JSON.stringify(result)),
            },
            "*",
          );
        } catch (error) {
          event.source?.postMessage(
            {
              type: "ADAPT_RESULT",
              cmd,
              requestId, // 返回 requestId 以便匹配并发请求
              error: String(error),
            },
            "*",
          );
        }
      }
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, []);

  // 获取应用图标
  const getAppIcon = (appId: string) =>
    apps.find((a) => a.id === appId)?.icon_url;

  // URL 代理转换 - 直接使用原始 URL
  const getProxiedUrl = useCallback((url: string) => {
    return url;
  }, []);

  // 启动或切换 PWA
  const launchOrSwitchPwa = useCallback(
    (app: AppInfo) => {
      const existing = runningPwas.find((p) => p.appId === app.id);

      if (existing) {
        setActivePwaId(app.id);
        setRunningPwas((prev) =>
          prev.map((p) =>
            p.appId === app.id ? { ...p, lastAccessed: Date.now() } : p,
          ),
        );
        return;
      }

      const snapshot = snapshots[app.id];

      if (runningPwas.length >= MAX_IFRAMES) {
        const lruPwa = [...runningPwas].sort(
          (a, b) => a.lastAccessed - b.lastAccessed,
        )[0];
        const iframe = iframesRef.current[lruPwa.appId];
        let scrollY = 0;
        try {
          scrollY = iframe?.contentWindow?.scrollY || 0;
        } catch (e) {}

        setSnapshots((prev) => ({
          ...prev,
          [lruPwa.appId]: {
            appId: lruPwa.appId,
            url: lruPwa.url,
            name: lruPwa.name,
            scrollY,
            timestamp: Date.now(),
          },
        }));

        if (iframesRef.current[lruPwa.appId]) {
          delete iframesRef.current[lruPwa.appId];
        }
        setRunningPwas((prev) => prev.filter((p) => p.appId !== lruPwa.appId));
        showMessage("success", `${lruPwa.name} 已暂停`);
      }

      const newPwa: RunningPwa = {
        appId: app.id,
        url: snapshot?.url || app.url,
        name: app.name,
        lastAccessed: Date.now(),
        scrollY: snapshot?.scrollY,
      };

      setRunningPwas((prev) => [...prev, newPwa]);
      setActivePwaId(app.id);

      if (snapshot) {
        setRestoringPwa(app.id);
        setTimeout(() => setRestoringPwa(null), 3000);
        setSnapshots((prev) => {
          const { [app.id]: _, ...rest } = prev;
          return rest;
        });
      }
    },
    [runningPwas, snapshots],
  );

  // iframe 加载完成
  const handleIframeLoad = useCallback(
    (appId: string) => {
      const pwa = runningPwas.find((p) => p.appId === appId);
      if (pwa?.scrollY && pwa.scrollY > 0) {
        const iframe = iframesRef.current[appId];
        try {
          iframe?.contentWindow?.scrollTo(0, pwa.scrollY);
        } catch (e) {}
      }
    },
    [runningPwas],
  );

  // 关闭 PWA
  const closePwa = useCallback(
    (appId: string) => {
      const pwa = runningPwas.find((p) => p.appId === appId);
      if (pwa) {
        const iframe = iframesRef.current[appId];
        let scrollY = 0;
        try {
          scrollY = iframe?.contentWindow?.scrollY || 0;
        } catch (e) {}

        setSnapshots((prev) => ({
          ...prev,
          [appId]: {
            appId,
            url: pwa.url,
            name: pwa.name,
            scrollY,
            timestamp: Date.now(),
          },
        }));

        if (iframesRef.current[appId]) {
          delete iframesRef.current[appId];
        }
      }

      const newRunning = runningPwas.filter((p) => p.appId !== appId);
      setRunningPwas(newRunning);

      if (activePwaId === appId) {
        if (newRunning.length > 0) {
          setActivePwaId(newRunning[newRunning.length - 1].appId);
        } else {
          setActivePwaId(null);
          setViewMode("apps");
        }
      }
    },
    [runningPwas, activePwaId],
  );

  // 刷新 PWA
  const refreshPwa = useCallback((appId: string) => {
    const iframe = iframesRef.current[appId];
    if (iframe) {
      iframe.src = iframe.src;
      showMessage("success", "页面已刷新");
    }
  }, []);

  // 卸载应用
  const handleUninstall = async (appId: string) => {
    if (!confirm("确定要卸载这个应用吗？")) return;

    try {
      if (runningPwas.find((p) => p.appId === appId)) {
        closePwa(appId);
      }
      setSnapshots((prev) => {
        const { [appId]: _, ...rest } = prev;
        return rest;
      });

      const response = await invoke<CommandResponse<boolean>>("uninstall_pwa", {
        appId,
      });
      if (response.success) {
        showMessage("success", "应用已卸载");
        loadApps();
      }
    } catch (error) {
      showMessage("error", `卸载失败：${error}`);
    }
  };

  // 打开浏览器
  const openBrowser = (url?: string) => {
    if (url) {
      setBrowserUrl(url);
      setBrowserHistory((prev) =>
        [
          { url, title: url, timestamp: Date.now() },
          ...prev.filter((h) => h.url !== url),
        ].slice(0, 50),
      );
    }
    setViewMode("browser");
    setActivePwaId(null);
  };

  return (
    <div className="app">
      {message && (
        <div className={`message ${message.type}`}>{message.text}</div>
      )}

      <main className={`main ${viewMode !== "apps" ? "with-content" : ""}`}>
        {viewMode === "browser" && (
          <BrowserView
            browserUrl={browserUrl}
            setBrowserUrl={setBrowserUrl}
            browserHistory={browserHistory}
            setBrowserHistory={setBrowserHistory}
            browserBookmarks={browserBookmarks}
            setBrowserBookmarks={setBrowserBookmarks}
            onClose={() => setViewMode("apps")}
            showMessage={showMessage}
          />
        )}

        {/* PWA iframe 容器 - 使用 CSS 隐藏而不是卸载，保持后台运行 */}
        <div
          className="iframe-container"
          style={{
            display: viewMode === "pwa" ? "block" : "none",
            visibility: viewMode === "pwa" ? "visible" : "hidden",
          }}
        >
          {runningPwas.map((pwa) => (
            <div
              key={pwa.appId}
              className={`iframe-wrapper ${activePwaId === pwa.appId ? "active" : ""}`}
              style={{
                display: activePwaId === pwa.appId ? "block" : "none",
              }}
            >
              {restoringPwa === pwa.appId && (
                <div className="restoring-overlay">
                  <div className="restoring-content">
                    <div className="spinner"></div>
                    <span>正在恢复 {pwa.name}...</span>
                  </div>
                </div>
              )}
              <iframe
                ref={(el) => {
                  if (el) iframesRef.current[pwa.appId] = el;
                }}
                src={getProxiedUrl(pwa.url)}
                sandbox="allow-scripts allow-same-origin allow-popups allow-forms allow-downloads allow-modals"
                allow="fullscreen; clipboard-write; autoplay"
                onLoad={() => handleIframeLoad(pwa.appId)}
                title={pwa.name}
              />
            </div>
          ))}

          {/* 悬浮切换按钮 */}
          <div className="floating-switcher right">
            {/* 按钮：面板隐藏时显示 */}
            {!showSwitcherPanel && (
              <button
                className="fab"
                onClick={() => setShowSwitcherPanel(true)}
                title="显示运行中的应用"
              >
                <span className="fab-indicator"></span>
                <span>{runningPwas.length}</span>
              </button>
            )}
            {/* 面板 */}
            {showSwitcherPanel && (
              <div className="switcher-panel">
                <div className="panel-header">
                  <span>运行中的应用 ({runningPwas.length})</span>
                  <div style={{ display: "flex", gap: "8px" }}>
                    <button
                      className="btn-manage"
                      onClick={() => {
                        setViewMode("apps");
                        setActivePwaId(null);
                        setShowSwitcherPanel(false);
                      }}
                    >
                      📋 管理
                    </button>
                    <button
                      className="btn-close"
                      onClick={() => setShowSwitcherPanel(false)}
                      title="关闭"
                    >
                      ✕
                    </button>
                  </div>
                </div>
                <div className="running-list">
                  {runningPwas.map((pwa) => (
                    <div
                      key={pwa.appId}
                      className={`running-item ${activePwaId === pwa.appId ? "active" : ""}`}
                      onClick={() => {
                        setActivePwaId(pwa.appId);
                        setRunningPwas((prev) =>
                          prev.map((p) =>
                            p.appId === pwa.appId
                              ? { ...p, lastAccessed: Date.now() }
                              : p,
                          ),
                        );
                      }}
                    >
                      <div className="item-icon">
                        {getAppIcon(pwa.appId) ? (
                          <img src={getAppIcon(pwa.appId)} alt={pwa.name} />
                        ) : (
                          "📱"
                        )}
                      </div>
                      <div className="item-info">
                        <span className="item-name">{pwa.name}</span>
                        <span className="item-status">
                          {activePwaId === pwa.appId ? "当前" : "后台"}
                        </span>
                      </div>
                      <div className="item-actions">
                        <button
                          className="btn-refresh-item"
                          onClick={(e) => {
                            e.stopPropagation();
                            refreshPwa(pwa.appId);
                          }}
                          title="刷新"
                        >
                          ↻
                        </button>
                        <button
                          className="btn-close-item"
                          onClick={(e) => {
                            e.stopPropagation();
                            closePwa(pwa.appId);
                          }}
                          title="关闭"
                        >
                          ✕
                        </button>
                      </div>
                    </div>
                  ))}
                  {Object.values(snapshots).map((snapshot) => (
                    <div
                      key={snapshot.appId}
                      className="running-item snapshot"
                      onClick={() => {
                        const app = apps.find((a) => a.id === snapshot.appId);
                        if (app) {
                          launchOrSwitchPwa(app);
                          setViewMode("pwa");
                        }
                      }}
                    >
                      <div className="item-icon">
                        {getAppIcon(snapshot.appId) ? (
                          <img
                            src={getAppIcon(snapshot.appId)}
                            alt={snapshot.name}
                          />
                        ) : (
                          "💤"
                        )}
                      </div>
                      <div className="item-info">
                        <span className="item-name">{snapshot.name}</span>
                        <span className="item-status">已暂停 (点击恢复)</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>

        {viewMode === "apps" && (
          <AppList
            apps={apps}
            runningPwas={runningPwas}
            snapshots={snapshots}
            loadApps={loadApps}
            showMessage={showMessage}
            setViewMode={setViewMode}
            openBrowser={openBrowser}
            launchOrSwitchPwa={launchOrSwitchPwa}
            handleUninstall={handleUninstall}
          />
        )}
      </main>

      <ProxySettings
        show={showProxy}
        onClose={() => setShowProxy(false)}
        showMessage={showMessage}
      />

      <AppSettings
        show={showAppSettings}
        onClose={() => setShowAppSettings(false)}
        showMessage={showMessage}
      />

      {viewMode === "apps" && (
        <div
          style={{
            position: "fixed",
            bottom: "100px",
            right: "20px",
            display: "flex",
            flexDirection: "column",
            gap: "16px",
            zIndex: 1000,
          }}
        >
          <button
            onClick={() => setShowAppSettings(true)}
            title="应用设置"
            style={{
              width: "50px",
              height: "50px",
              borderRadius: "50%",
              border: "none",
              background: "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)",
              color: "white",
              fontSize: "20px",
              cursor: "pointer",
              boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            ⚙️
          </button>
          <button
            onClick={() => setShowProxy(true)}
            title="代理设置"
            style={{
              width: "50px",
              height: "50px",
              borderRadius: "50%",
              border: "none",
              background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
              color: "white",
              fontSize: "20px",
              cursor: "pointer",
              boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            🔧
          </button>
        </div>
      )}

      <Test />
      <footer className="footer">
        <p>
          PWA Container v0.1.0 - {runningPwas.length}/{MAX_IFRAMES} 运行中
        </p>
      </footer>
    </div>
  );
}

export default App;
