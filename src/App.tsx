import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./styles/App.css";
import "../adapt.min.js";
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

const MAX_IFRAMES = 6;

function App() {
  // 视图模式
  const [viewMode, setViewMode] = useState<ViewMode>('apps');

  // 浏览器状态
  const [browserUrl, setBrowserUrl] = useState('');
  const [browserHistory, setBrowserHistory] = useState<BrowserHistoryItem[]>([]);

  // 应用状态
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [runningPwas, setRunningPwas] = useState<RunningPwa[]>([]);
  const [activePwaId, setActivePwaId] = useState<string | null>(null);
  const [snapshots, setSnapshots] = useState<Record<string, PwaSnapshot>>({});
  const [restoringPwa, setRestoringPwa] = useState<string | null>(null);

  // 代理设置
  const [showProxy, setShowProxy] = useState(false);

  // 消息提示
  const [message, setMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);

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

      const iframe = Object.values(iframesRef.current).find(
        (f) => f.contentWindow === event.source
      );
      if (!iframe) return;

      if (event.data?.type === "ADAPT_INVOKE") {
        const { id, cmd, payload } = event.data;
        try {
          const result = await invoke(cmd, payload);
          event.source?.postMessage(
            { type: "ADAPT_RESPONSE", id, result: JSON.parse(JSON.stringify(result)) },
            "*"
          );
        } catch (error) {
          event.source?.postMessage(
            { type: "ADAPT_RESPONSE", id, error: String(error) },
            "*"
          );
        }
      }
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, []);

  // 获取应用图标
  const getAppIcon = (appId: string) => apps.find((a) => a.id === appId)?.icon_url;

  // URL 代理转换 - 直接使用原始 URL，依赖 WebView 自带缓存
  const getProxiedUrl = (url: string) => {
    return url;
  };

  // 启动或切换 PWA
  const launchOrSwitchPwa = useCallback((app: AppInfo) => {
    const existing = runningPwas.find((p) => p.appId === app.id);

    if (existing) {
      setActivePwaId(app.id);
      setRunningPwas((prev) =>
        prev.map((p) => (p.appId === app.id ? { ...p, lastAccessed: Date.now() } : p))
      );
      return;
    }

    const snapshot = snapshots[app.id];

    if (runningPwas.length >= MAX_IFRAMES) {
      const lruPwa = [...runningPwas].sort((a, b) => a.lastAccessed - b.lastAccessed())[0];
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
  }, [runningPwas, snapshots]);

  // iframe 加载完成
  const handleIframeLoad = (appId: string) => {
    const pwa = runningPwas.find((p) => p.appId === appId);
    if (pwa?.scrollY && pwa.scrollY > 0) {
      const iframe = iframesRef.current[appId];
      try {
        iframe?.contentWindow?.scrollTo(0, pwa.scrollY);
      } catch (e) {}
    }
  };

  // 关闭 PWA
  const closePwa = (appId: string) => {
    const pwa = runningPwas.find((p) => p.appId === appId);
    if (pwa) {
      const iframe = iframesRef.current[appId];
      let scrollY = 0;
      try {
        scrollY = iframe?.contentWindow?.scrollY || 0;
      } catch (e) {}

      setSnapshots((prev) => ({
        ...prev,
        [appId]: { appId, url: pwa.url, name: pwa.name, scrollY, timestamp: Date.now() },
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
        setViewMode('apps');
      }
    }
  };

  // 刷新 PWA
  const refreshPwa = (appId: string) => {
    const iframe = iframesRef.current[appId];
    if (iframe) {
      iframe.src = iframe.src;
      showMessage("success", "页面已刷新");
    }
  };

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

      const response = await invoke<CommandResponse<boolean>>("uninstall_pwa", { appId });
      if (response.success) {
        showMessage("success", "应用已卸载");
        loadApps();
      }
    } catch (error) {
      showMessage("error", `卸载失败：${error}`);
    }
  };

  // 更新应用
  const handleUpdate = async (appId: string) => {
    if (!confirm("确定要清理本地缓存并检查更新吗？")) return;

    try {
      const response = await invoke<CommandResponse<boolean>>("update_pwa", { appId });
      if (response.success) {
        showMessage("success", "缓存已清理，下次启动将加载最新资源");
      }
    } catch (error) {
      showMessage("error", `清理失败：${error}`);
    }
  };

  // 打开浏览器
  const openBrowser = (url?: string) => {
    if (url) {
      setBrowserUrl(url);
      setBrowserHistory((prev) =>
        [{ url, title: url, timestamp: Date.now() }, ...prev.filter((h) => h.url !== url)].slice(0, 50)
      );
    }
    setViewMode('browser');
    setActivePwaId(null);
  };

  return (
    <div className="app">
      {message && <div className={`message ${message.type}`}>{message.text}</div>}

      <main className={`main ${viewMode !== 'apps' ? "with-content" : ""}`}>
        {viewMode === 'browser' && (
          <BrowserView
            browserUrl={browserUrl}
            setBrowserUrl={setBrowserUrl}
            browserHistory={browserHistory}
            setBrowserHistory={setBrowserHistory}
            onClose={() => setViewMode('apps')}
            getProxiedUrl={getProxiedUrl}
            showMessage={showMessage}
          />
        )}

        {viewMode === 'pwa' && (
          <div className="iframe-container">
            {runningPwas.map((pwa) => (
              <div
                key={pwa.appId}
                className={`iframe-wrapper ${activePwaId === pwa.appId ? "active" : ""}`}
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
                  ref={(el) => { if (el) iframesRef.current[pwa.appId] = el; }}
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
              <button 
                className="fab" 
                onClick={() => {
                  const panel = document.querySelector('.switcher-panel') as HTMLElement;
                  if (panel) panel.style.display = panel.style.display === 'none' ? 'block' : 'none';
                }}
              >
                <span className="fab-indicator"></span>
                <span>{runningPwas.length}</span>
              </button>
              <div className="switcher-panel" style={{ display: 'block' }}>
                <div className="panel-header">
                  <span>运行中的应用 ({runningPwas.length})</span>
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <button
                      className="btn-manage"
                      onClick={() => {
                        setViewMode('apps');
                        setActivePwaId(null);
                      }}
                    >
                      📋 管理
                    </button>
                    <button
                      className="btn-close"
                      onClick={() => {
                        const panel = document.querySelector('.switcher-panel') as HTMLElement;
                        if (panel) panel.style.display = 'none';
                      }}
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
                          prev.map((p) => (p.appId === pwa.appId ? { ...p, lastAccessed: Date.now() } : p))
                        );
                      }}
                    >
                      <div className="item-icon">{getAppIcon(pwa.appId) ? <img src={getAppIcon(pwa.appId)} alt={pwa.name} /> : "📱"}</div>
                      <div className="item-info">
                        <span className="item-name">{pwa.name}</span>
                        <span className="item-status">{activePwaId === pwa.appId ? "当前" : "后台"}</span>
                      </div>
                      <div className="item-actions">
                        <button onClick={(e) => { e.stopPropagation(); refreshPwa(pwa.appId); }} title="刷新">↻</button>
                        <button onClick={(e) => { e.stopPropagation(); closePwa(pwa.appId); }} title="关闭">✕</button>
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
                          setViewMode('pwa');
                        }
                      }}
                    >
                      <div className="item-icon">{getAppIcon(snapshot.appId) ? <img src={getAppIcon(snapshot.appId)} alt={snapshot.name} /> : "💤"}</div>
                      <div className="item-info">
                        <span className="item-name">{snapshot.name}</span>
                        <span className="item-status">已暂停 (点击恢复)</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {viewMode === 'apps' && (
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
            handleUpdate={handleUpdate}
          />
        )}
      </main>

      <ProxySettings show={showProxy} onClose={() => setShowProxy(false)} showMessage={showMessage} />

      {viewMode === 'apps' && (
        <button className="proxy-settings-btn" onClick={() => setShowProxy(true)} title="代理设置">
          🔧
        </button>
      )}

      <footer className="footer">
        <p>PWA Container v0.1.0 - {runningPwas.length}/{MAX_IFRAMES} 运行中</p>
      </footer>
    </div>
  );
}

export default App;