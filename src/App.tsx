import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./styles/App.css";
import {
  BrowserView,
  AppList,
  ProxySettings,
  type AppInfo,
  type RunningPwa,
  type PwaSnapshot,
  type CommandResponse,
  type ViewMode,
  type BrowserHistoryItem,
} from "./components";
import Test from "./Test.js";

const MAX_IFRAMES = 6;

function App() {
  // 视图模式
  const [viewMode, setViewMode] = useState<ViewMode>("apps");

  // 浏览器状态
  const [browserUrl, setBrowserUrl] = useState("");
  const [browserHistory, setBrowserHistory] = useState<BrowserHistoryItem[]>(
    [],
  );

  // 应用状态
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [runningPwas, setRunningPwas] = useState<RunningPwa[]>([]);
  const [activePwaId, setActivePwaId] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState<Record<string, PwaSnapshot>>({});
  const [restoringPwa, setRestoringPwa] = useState<string | null>(null);

  // 代理设置
  const [showProxy, setShowProxy] = useState(false);

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
        const { requestId, url, method, headers, body, isMedia } = event.data;
        console.log("[App] Proxy request:", {
          requestId,
          url,
          method,
          isMedia,
          hasHeaders: !!headers,
          hasBody: !!body,
        });
        try {
          // 把所有请求参数放在 body 里传给代理服务器
          // 父窗口的 fetch 只设置 Content-Type，不设置自定义 headers（避免浏览器拦截）
          const proxyBodyObj = {
            target: url,
            method: method || "GET",
            headers: headers || {},
            body: body || null,
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

          const responseData = {
            status: proxyResponse.status,
            statusText: proxyResponse.statusText,
            headers: Object.fromEntries(proxyResponse.headers.entries()),
            body: await proxyResponse.text(),
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
        const { cmd, payload } = event.data;
        try {
          const result = await invoke(cmd, payload);
          event.source?.postMessage(
            {
              type: "ADAPT_RESULT",
              cmd,
              result: JSON.parse(JSON.stringify(result)),
            },
            "*",
          );
        } catch (error) {
          event.source?.postMessage(
            { type: "ADAPT_RESULT", cmd, error: String(error) },
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

  // URL 代理转换 - 直接使用原始 URL，依赖 WebView 自带缓存
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

      {viewMode === "apps" && (
        <button
          className="proxy-settings-btn"
          onClick={() => setShowProxy(true)}
          title="代理设置"
        >
          🔧
        </button>
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
