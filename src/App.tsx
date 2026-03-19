import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./styles/App.css";
import {
  BrowserView,
  AppList,
  ProxySettings,
  AppSettings,
  DataManager,
  AppDataManager,
  ConfirmDialogProvider,
  confirmDialog,
  type AppInfo,
  type RunningPwa,
  type PwaSnapshot,
  type ViewMode,
  type BrowserHistoryItem,
  type BrowserBookmarkItem,
} from "./components";
import Test from "./Test.js";
import { listApps, uninstallPwa, setRunningPwas } from "./pwa";
import { kvGet, kvSet } from "./kv";
import { usePostMessage } from "./hooks";

const MAX_IFRAMES = 6;

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

  // 从数据库加载浏览器数据
  useEffect(() => {
    const loadBrowserData = async () => {
      try {
        const historyJson = await kvGet("browser", "history");
        if (historyJson) {
          setBrowserHistory(JSON.parse(historyJson));
        }

        const bookmarksJson = await kvGet("browser", "bookmarks");
        if (bookmarksJson) {
          setBrowserBookmarks(JSON.parse(bookmarksJson));
        }
      } catch (e) {
        console.error("Failed to load browser data from DB:", e);
      } finally {
        setBrowserDataLoaded(true);
      }
    };
    loadBrowserData();
  }, []);

  // 保存历史到数据库
  useEffect(() => {
    if (!browserDataLoaded) return;
    const saveHistory = async () => {
      try {
        await kvSet("browser", "history", JSON.stringify(browserHistory));
      } catch (e) {
        console.error("Failed to save history to DB:", e);
      }
    };
    saveHistory();
  }, [browserHistory, browserDataLoaded]);

  // 保存收藏到数据库
  useEffect(() => {
    if (!browserDataLoaded) return;
    const saveBookmarks = async () => {
      try {
        await kvSet("browser", "bookmarks", JSON.stringify(browserBookmarks));
      } catch (e) {
        console.error("Failed to save bookmarks to DB:", e);
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

  // 数据管理
  const [showDataManager, setShowDataManager] = useState(false);

  // 单个应用数据管理
  const [selectedAppForData, setSelectedAppForData] = useState<AppInfo | null>(
    null,
  );

  // 消息提示
  const [message, setMessage] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // 悬浮面板显示状态
  const [showSwitcherPanel, setShowSwitcherPanel] = useState(false);

  // 显示消息（必须在 usePostMessage 之前定义）
  const showMessage = useCallback((type: "success" | "error", text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  }, []);

  // 使用 postMessage hook
  const { iframesRef } = usePostMessage({ apps, showMessage });

  // 加载应用列表（使用 pwa.ts）
  const loadApps = useCallback(async () => {
    try {
      const apps = await listApps();
      setApps(apps);
    } catch (error) {
      showMessage("error", `加载应用列表失败：${error}`);
    }
  }, [showMessage]);

  // 初始化
  useEffect(() => {
    loadApps();

    // 加载屏幕常亮设置
    const loadKeepScreenOn = async () => {
      try {
        const enabledStr = await kvGet("config", "keep_screen_on");
        if (enabledStr === "true") {
          await invoke("set_keep_screen_on", { enabled: true });
        }
      } catch (e) {
        console.error("Failed to load keep screen on setting:", e);
      }
    };
    loadKeepScreenOn();
  }, [loadApps]);

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
    const confirmed = await confirmDialog({
      title: "卸载应用",
      message: "确定要卸载这个应用吗？",
      isDanger: true,
    });
    if (!confirmed) return;

    try {
      if (runningPwas.find((p) => p.appId === appId)) {
        closePwa(appId);
      }
      setSnapshots((prev) => {
        const { [appId]: _, ...rest } = prev;
        return rest;
      });

      await uninstallPwa(appId);
      showMessage("success", "应用已卸载");
      loadApps();
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
    <ConfirmDialogProvider>
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
                referrerPolicy="no-referrer"
                onLoad={() => handleIframeLoad(pwa.appId)}
                onError={(e) => {
                  console.error(
                    `[App] Failed to load iframe for ${pwa.appId}:`,
                    e,
                  );
                }}
                title={pwa.name}
                style={{ width: "100%", height: "100%", border: "none" }}
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
            onManageData={(app) => setSelectedAppForData(app)}
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

      <DataManager
        show={showDataManager}
        onClose={() => setShowDataManager(false)}
        showMessage={showMessage}
      />

      <AppDataManager
        app={selectedAppForData}
        show={!!selectedAppForData}
        onClose={() => setSelectedAppForData(null)}
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
          <button
            onClick={() => setShowDataManager(true)}
            title="数据管理"
            style={{
              width: "50px",
              height: "50px",
              borderRadius: "50%",
              border: "none",
              background: "linear-gradient(135deg, #f093fb 0%, #f5576c 100%)",
              color: "white",
              fontSize: "20px",
              cursor: "pointer",
              boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            🗑️
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
    </ConfirmDialogProvider>
  );
}

export default App;
