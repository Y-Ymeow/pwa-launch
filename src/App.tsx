import { useState, useEffect, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import "./styles/App.css";
import Test from "./Test";

// 可拖动的悬浮切换按钮组件
interface DraggableSwitcherProps {
  runningCount: number;
  maxCount: number;
  showSwitcher: boolean;
  setShowSwitcher: (show: boolean) => void;
  children: React.ReactNode;
}

function DraggableSwitcher({
  runningCount,
  maxCount,
  showSwitcher,
  setShowSwitcher,
  children,
}: DraggableSwitcherProps) {
  const [position, setPosition] = useState<"left" | "right">("right");
  const [isDragging, setIsDragging] = useState(false);
  const [startY, setStartY] = useState(0);
  const [currentY, setCurrentY] = useState(0);
  const switcherRef = useRef<HTMLDivElement>(null);

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    setStartY(e.clientY - currentY);
  };

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isDragging) return;
      const newY = e.clientY - startY;
      setCurrentY(Math.max(-200, Math.min(newY, 200)));
    },
    [isDragging, startY],
  );

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  const handleTouchStart = (e: React.TouchEvent) => {
    setIsDragging(true);
    setStartY(e.touches[0].clientY - currentY);
  };

  const handleTouchMove = useCallback(
    (e: TouchEvent) => {
      if (!isDragging) return;
      const newY = e.touches[0].clientY - startY;
      setCurrentY(Math.max(-200, Math.min(newY, 200)));
    },
    [isDragging, startY],
  );

  const handleTouchEnd = useCallback(() => {
    setIsDragging(false);
  }, []);

  useEffect(() => {
    if (isDragging) {
      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
      window.addEventListener("touchmove", handleTouchMove);
      window.addEventListener("touchend", handleTouchEnd);
    }
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      window.removeEventListener("touchmove", handleTouchMove);
      window.removeEventListener("touchend", handleTouchEnd);
    };
  }, [
    isDragging,
    handleMouseMove,
    handleMouseUp,
    handleTouchMove,
    handleTouchEnd,
  ]);

  const togglePosition = () => {
    setPosition((prev) => (prev === "right" ? "left" : "right"));
  };

  return (
    <div
      ref={switcherRef}
      className={`floating-switcher ${position} ${showSwitcher ? "" : "hidden"}`}
      style={{ transform: `translateY(calc(-50% + ${currentY}px))` }}
    >
      <button
        className="fab"
        onMouseDown={handleMouseDown}
        onTouchStart={handleTouchStart}
        onClick={() => !isDragging && setShowSwitcher(!showSwitcher)}
        onContextMenu={(e) => {
          e.preventDefault();
          togglePosition();
        }}
      >
        <span className="fab-indicator"></span>
        <span>
          {runningCount}/{maxCount}
        </span>
      </button>
      {children}
    </div>
  );
}

interface AppInfo {
  id: string;
  name: string;
  url: string;
  icon_url?: string;
  installed_at: number;
  display_mode: string;
}

interface CommandResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// 运行的PWA状态
interface RunningPwa {
  appId: string;
  url: string;
  name: string;
  lastAccessed: number; // LRU时间戳
  scrollY?: number; // 保存的滚动位置
}

// 已销毁但需要恢复状态的PWA
interface PwaSnapshot {
  appId: string;
  url: string;
  name: string;
  scrollY: number;
  timestamp: number;
}

const MAX_IFRAMES = 4; // 最多4个iframe

// 代理设置类型
interface ProxySettings {
  enabled: boolean;
  proxy_type: "http" | "https" | "socks5";
  host: string;
  port: number;
  username: string;
  password: string;
}

function App() {
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [installUrl, setInstallUrl] = useState("");
  const [installing, setInstalling] = useState(false);
  const [message, setMessage] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // 运行的PWA（最多4个有iframe）
  const [runningPwas, setRunningPwas] = useState<RunningPwa[]>([]);

  // 代理设置
  const [showProxySettings, setShowProxySettings] = useState(false);
  const [proxySettings, setProxySettings] = useState<ProxySettings>({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 8080,
    username: "",
    password: "",
  });
  // 当前激活的PWA
  const [activePwaId, setActivePwaId] = useState<string | null>(null);
  // 快照（已销毁的PWA状态）
  const [snapshots, setSnapshots] = useState<Record<string, PwaSnapshot>>({});
  // 是否显示切换面板
  const [showSwitcher, setShowSwitcher] = useState(false);
  // 恢复中的PWA
  const [restoringPwa, setRestoringPwa] = useState<string | null>(null);

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

  // 加载代理设置
  const loadProxySettings = async () => {
    try {
      const response = await invoke<CommandResponse<ProxySettings | null>>("get_proxy");
      if (response.success && response.data) {
        setProxySettings(response.data);
      }
    } catch (error) {
      console.error("加载代理设置失败:", error);
    }
  };

  // 保存代理设置
  const saveProxySettings = async () => {
    try {
      await invoke("set_proxy", {
        enabled: proxySettings.enabled,
        proxyType: proxySettings.proxy_type,
        host: proxySettings.host,
        port: proxySettings.port,
        username: proxySettings.username || null,
        password: proxySettings.password || null,
      });
      showMessage("success", "代理设置已保存");
      setShowProxySettings(false);
    } catch (error) {
      showMessage("error", `保存代理设置失败：${error}`);
    }
  };

  // 测试代理连接
  const testProxy = async () => {
    try {
      // 先临时保存设置
      await invoke("set_proxy", {
        enabled: proxySettings.enabled,
        proxyType: proxySettings.proxy_type,
        host: proxySettings.host,
        port: proxySettings.port,
        username: proxySettings.username || null,
        password: proxySettings.password || null,
      });
      
      // 测试请求
      const response = await invoke<CommandResponse<{status: number}>>("proxy_fetch", {
        url: "http://httpbin.org/ip",
        method: "GET",
        headers: {},
        body: null,
      });
      
      if (response.success) {
        showMessage("success", `代理测试成功！状态码: ${response.data?.status}`);
      } else {
        showMessage("error", "代理测试失败");
      }
    } catch (error) {
      showMessage("error", `代理测试失败：${error}`);
    }
  };

  useEffect(() => {
    loadApps();
    loadProxySettings();

    // 全局监听来自 iframe 的 adapt 请求
    const handleMessage = async (event: MessageEvent) => {
      // 只处理来自 iframe 的消息
      const iframe = Object.values(iframesRef.current).find(
        (f) => f.contentWindow === event.source,
      );
      if (!iframe) return;

      if (event.data?.type === "ADAPT_INVOKE") {
        const { id, cmd, payload } = event.data;
        try {
          const result = await invoke(cmd, payload);

          // 序列化确保只传递可克隆的数据
          const serialized = JSON.parse(JSON.stringify(result));
          event.source?.postMessage(
            {
              type: "ADAPT_RESPONSE",
              id,
              result: serialized,
            },
            "*",
          );
        } catch (error) {
          event.source?.postMessage(
            {
              type: "ADAPT_RESPONSE",
              id,
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

  // 安装 PWA
  const handleInstall = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!installUrl.trim()) return;

    setInstalling(true);
    try {
      const response = await invoke<CommandResponse<AppInfo>>("install_pwa", {
        request: { url: installUrl.trim() },
      });

      if (response.success && response.data) {
        showMessage("success", `应用 "${response.data.name}" 安装成功！`);
        setInstallUrl("");
        loadApps();
      } else {
        showMessage("error", response.error || "安装失败");
      }
    } catch (error) {
      showMessage("error", `安装失败：${error}`);
    } finally {
      setInstalling(false);
    }
  };

  // 获取iframe的sandbox属性
  const getIframeSandbox = () => {
    return "allow-scripts allow-same-origin allow-popups allow-forms allow-downloads allow-modals";
  };

  // 启动或切换到PWA（LRU管理）
  const launchOrSwitchPwa = useCallback(
    (app: AppInfo) => {
      const existing = runningPwas.find((p) => p.appId === app.id);

      if (existing) {
        // 已运行，直接切换
        setActivePwaId(app.id);
        // 更新访问时间
        setRunningPwas((prev) =>
          prev.map((p) =>
            p.appId === app.id ? { ...p, lastAccessed: Date.now() } : p,
          ),
        );
        setShowSwitcher(false);
        return;
      }

      // 检查是否有快照需要恢复
      const snapshot = snapshots[app.id];

      // 检查是否超过4个
      if (runningPwas.length >= MAX_IFRAMES) {
        // 找到最久未使用的（LRU）
        const lruPwa = [...runningPwas].sort(
          (a, b) => a.lastAccessed - b.lastAccessed,
        )[0];

        // 获取iframe的滚动位置
        const iframe = iframesRef.current[lruPwa.appId];
        let scrollY = 0;
        if (iframe?.contentWindow) {
          try {
            scrollY = iframe.contentWindow.scrollY || 0;
          } catch (e) {
            // 跨域可能无法访问
          }
        }

        // 保存快照
        const newSnapshot: PwaSnapshot = {
          appId: lruPwa.appId,
          url: lruPwa.url,
          name: lruPwa.name,
          scrollY,
          timestamp: Date.now(),
        };
        setSnapshots((prev) => ({ ...prev, [lruPwa.appId]: newSnapshot }));

        // 从DOM移除iframe（真正释放内存）
        if (iframesRef.current[lruPwa.appId]) {
          delete iframesRef.current[lruPwa.appId];
        }

        // 从运行列表移除
        setRunningPwas((prev) => prev.filter((p) => p.appId !== lruPwa.appId));

        showMessage("success", `${lruPwa.name} 已暂停运行，切换到 ${app.name}`);
      }

      // 添加新的PWA到运行列表
      const newPwa: RunningPwa = {
        appId: app.id,
        url: snapshot?.url || app.url,
        name: app.name,
        lastAccessed: Date.now(),
        scrollY: snapshot?.scrollY,
      };

      setRunningPwas((prev) => [...prev, newPwa]);
      setActivePwaId(app.id);
      setShowSwitcher(false);

      // 如果有快照，标记为恢复中
      if (snapshot) {
        setRestoringPwa(app.id);
        // 3秒后清除恢复状态
        setTimeout(() => setRestoringPwa(null), 3000);
        // 删除已使用的快照
        setSnapshots((prev) => {
          const { [app.id]: _, ...rest } = prev;
          return rest;
        });
      }
    },
    [runningPwas, snapshots],
  );

  // iframe加载完成后恢复滚动位置
  const handleIframeLoad = (appId: string) => {
    const pwa = runningPwas.find((p) => p.appId === appId);
    if (pwa?.scrollY && pwa.scrollY > 0) {
      const iframe = iframesRef.current[appId];
      if (iframe?.contentWindow) {
        try {
          iframe.contentWindow.scrollTo(0, pwa.scrollY);
        } catch (e) {
          // 跨域可能无法操作
        }
      }
    }
  };

  // 关闭PWA
  const closePwa = (appId: string, e?: React.MouseEvent) => {
    e?.stopPropagation();

    const pwa = runningPwas.find((p) => p.appId === appId);
    if (pwa) {
      // 保存快照
      const iframe = iframesRef.current[appId];
      let scrollY = 0;
      if (iframe?.contentWindow) {
        try {
          scrollY = iframe.contentWindow.scrollY || 0;
        } catch (e) {}
      }

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

      // 从DOM移除
      if (iframesRef.current[appId]) {
        delete iframesRef.current[appId];
      }
    }

    // 从运行列表移除
    const newRunning = runningPwas.filter((p) => p.appId !== appId);
    setRunningPwas(newRunning);

    // 如果关闭的是当前激活的，切换到下一个
    if (activePwaId === appId) {
      if (newRunning.length > 0) {
        setActivePwaId(newRunning[newRunning.length - 1].appId);
      } else {
        setActivePwaId(null);
      }
    }
  };

  // 刷新PWA
  const refreshPwa = (appId: string, e?: React.MouseEvent) => {
    e?.stopPropagation();

    const iframe = iframesRef.current[appId];
    if (iframe) {
      // 记录当前URL
      const currentUrl = iframe.src;
      // 重新加载iframe
      iframe.src = currentUrl;
      showMessage("success", "页面已刷新");
    }
  };

  // 卸载应用
  const handleUninstall = async (appId: string) => {
    if (!confirm("确定要卸载这个应用吗？")) return;

    try {
      // 如果正在运行，先关闭
      if (runningPwas.find((p) => p.appId === appId)) {
        closePwa(appId);
      }
      // 删除快照
      setSnapshots((prev) => {
        const { [appId]: _, ...rest } = prev;
        return rest;
      });

      const response = await invoke<CommandResponse<boolean>>("uninstall_pwa", {
        appId,
      });
      if (response.success && response.data) {
        showMessage("success", "应用已卸载");
        loadApps();
      }
    } catch (error) {
      showMessage("error", `卸载失败：${error}`);
    }
  };

  const showMessage = (type: "success" | "error", text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString("zh-CN");
  };

  // 获取应用图标
  const getAppIcon = (appId: string) => {
    const app = apps.find((a) => a.id === appId);
    return app?.icon_url;
  };

  // 是否有活跃的PWA（显示iframe区域）
  const hasActivePwa = activePwaId && runningPwas.length > 0;

  return (
    <div className="app">
      {/* 消息提示 */}
      {message && (
        <div className={`message ${message.type}`}>{message.text}</div>
      )}

      {/* 主内容区 - 根据是否有活跃PWA切换布局 */}
      <main className={`main ${hasActivePwa ? "with-pwa" : ""}`}>
        {/* iframe 容器 - 显示运行的PWA */}
        <div
          className="iframe-container"
          style={{ display: hasActivePwa ? "block" : "none" }}
        >
          {runningPwas.map((pwa) => (
            <div
              key={pwa.appId}
              className={`iframe-wrapper ${activePwaId === pwa.appId ? "active" : ""}`}
            >
              {/* 恢复提示 */}
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
                src={pwa.url}
                sandbox={getIframeSandbox()}
                allow="fullscreen; clipboard-write; autoplay"
                onLoad={() => handleIframeLoad(pwa.appId)}
                title={pwa.name}
              />
            </div>
          ))}
        </div>

        {/* 应用管理区域 */}
        <div
          className="management-area"
          style={{ display: hasActivePwa ? "none" : "block" }}
        >
          <header className="header">
            <h1>🚀 PWA Container</h1>
            <p className="subtitle">最多同时运行 {MAX_IFRAMES} 个应用</p>
          </header>

          {/* 安装表单 */}
          <section className="install-section">
            <form onSubmit={handleInstall} className="install-form">
              <input
                type="url"
                value={installUrl}
                onChange={(e) => setInstallUrl(e.target.value)}
                placeholder="输入 PWA 应用 URL..."
                disabled={installing}
                required
              />
              <button
                type="submit"
                disabled={installing}
                className="btn-primary"
              >
                {installing ? "安装中..." : "安装应用"}
              </button>
            </form>
          </section>

          {/* 应用列表 */}
          <section className="apps-section">
            <h2>已安装的应用 ({apps.length})</h2>

            {apps.length === 0 ? (
              <div className="empty-state">
                <p>暂无应用，安装一个 PWA 应用开始使用吧！</p>
              </div>
            ) : (
              <div className="apps-grid">
                {apps.map((app) => {
                  const isRunning = runningPwas.find((p) => p.appId === app.id);
                  const hasSnapshot = snapshots[app.id];

                  return (
                    <div
                      key={app.id}
                      className={`app-card ${isRunning ? "running" : ""}`}
                    >
                      <div className="app-icon">
                        {app.icon_url ? (
                          <img
                            src={app.icon_url}
                            alt={app.name}
                            onError={(e) => {
                              (e.target as HTMLImageElement).style.display =
                                "none";
                              (
                                e.target as HTMLImageElement
                              ).parentElement!.innerHTML = "<span>📱</span>";
                            }}
                          />
                        ) : (
                          <span>📱</span>
                        )}
                      </div>
                      <h3>{app.name}</h3>
                      <p className="app-status">
                        {isRunning
                          ? "🟢 运行中"
                          : hasSnapshot
                            ? "💤 已暂停"
                            : "⚪ 未启动"}
                      </p>
                      <p className="app-date">
                        安装于：{formatDate(app.installed_at)}
                      </p>

                      <div className="app-actions">
                        <button
                          className="btn-launch"
                          onClick={() => launchOrSwitchPwa(app)}
                        >
                          {isRunning
                            ? "🔀 切换"
                            : hasSnapshot
                              ? "▶️ 恢复"
                              : "🚀 启动"}
                        </button>
                        <button
                          className="btn-danger"
                          onClick={() => handleUninstall(app.id)}
                        >
                          ❌ 卸载
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      </main>

      {/* 悬浮切换按钮 - 只在有运行PWA时显示 - 贴边可拖动 */}
      {hasActivePwa && (
        <DraggableSwitcher
          runningCount={runningPwas.length}
          maxCount={MAX_IFRAMES}
          showSwitcher={showSwitcher}
          setShowSwitcher={setShowSwitcher}
        >
          {showSwitcher && (
            <div className="switcher-panel">
              <div className="panel-header">
                <span>运行中的应用</span>
                <button
                  className="btn-manage"
                  onClick={() => {
                    setActivePwaId(null);
                    setShowSwitcher(false);
                  }}
                >
                  📋 管理全部
                </button>
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
                      setShowSwitcher(false);
                    }}
                  >
                    <div className="item-icon">
                      {getAppIcon(pwa.appId) ? (
                        <img src={getAppIcon(pwa.appId)} alt={pwa.name} />
                      ) : (
                        "📱"
                      )}
                    </div>{" "}
                    <div className="item-info">
                      <span className="item-name">{pwa.name}</span>
                      <span className="item-status">
                        {activePwaId === pwa.appId ? "当前" : "后台"}
                      </span>
                    </div>
                    <div className="item-actions">
                      <button
                        className="btn-refresh-item"
                        onClick={(e) => refreshPwa(pwa.appId, e)}
                        title="刷新页面"
                      >
                        ↻
                      </button>
                      <button
                        className="btn-close-item"
                        onClick={(e) => closePwa(pwa.appId, e)}
                        title="关闭应用"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                ))}

                {/* 显示已暂停的（有快照的） */}
                {Object.values(snapshots).map((snapshot) => (
                  <div
                    key={snapshot.appId}
                    className="running-item snapshot"
                    onClick={() => {
                      const app = apps.find((a) => a.id === snapshot.appId);
                      if (app) launchOrSwitchPwa(app);
                      setShowSwitcher(false);
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
                    </div>{" "}
                    <div className="item-info">
                      <span className="item-name">{snapshot.name}</span>
                      <span className="item-status">已暂停 (点击恢复)</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </DraggableSwitcher>
      )}

      {/* 代理设置按钮 - 只在主窗口显示，不在 iframe 中显示 */}
      {window.parent === window && (
        <button
          className="proxy-settings-btn"
          onClick={() => setShowProxySettings(true)}
          title="代理设置"
        >
          🔧
        </button>
      )}

      {/* 代理设置面板 */}
      {showProxySettings && (
        <div className="proxy-settings-modal" onClick={() => setShowProxySettings(false)}>
          <div className="proxy-settings-panel" onClick={(e) => e.stopPropagation()}>
            <div className="proxy-settings-header">
              <h3>代理设置</h3>
              <button onClick={() => setShowProxySettings(false)}>✕</button>
            </div>
            <div className="proxy-settings-body">
              <label className="proxy-enable-label">
                <input
                  type="checkbox"
                  checked={proxySettings.enabled}
                  onChange={(e) =>
                    setProxySettings({ ...proxySettings, enabled: e.target.checked })
                  }
                />
                启用代理
              </label>

              <div className="proxy-field">
                <label>代理类型：</label>
                <select
                  value={proxySettings.proxy_type}
                  onChange={(e) =>
                    setProxySettings({
                      ...proxySettings,
                      proxy_type: e.target.value as "http" | "https" | "socks5",
                    })
                  }
                >
                  <option value="http">HTTP</option>
                  <option value="https">HTTPS</option>
                  <option value="socks5">SOCKS5</option>
                </select>
              </div>

              <div className="proxy-field">
                <label>主机地址：</label>
                <input
                  type="text"
                  placeholder="127.0.0.1"
                  value={proxySettings.host}
                  onChange={(e) =>
                    setProxySettings({ ...proxySettings, host: e.target.value })
                  }
                />
              </div>

              <div className="proxy-field">
                <label>端口：</label>
                <input
                  type="number"
                  placeholder="8080"
                  value={proxySettings.port}
                  onChange={(e) =>
                    setProxySettings({
                      ...proxySettings,
                      port: parseInt(e.target.value) || 0,
                    })
                  }
                />
              </div>

              <div className="proxy-field">
                <label>用户名（可选）：</label>
                <input
                  type="text"
                  value={proxySettings.username}
                  onChange={(e) =>
                    setProxySettings({ ...proxySettings, username: e.target.value })
                  }
                />
              </div>

              <div className="proxy-field">
                <label>密码（可选）：</label>
                <input
                  type="password"
                  value={proxySettings.password}
                  onChange={(e) =>
                    setProxySettings({ ...proxySettings, password: e.target.value })
                  }
                />
              </div>

              <div className="proxy-actions">
                <button className="proxy-test-btn" onClick={testProxy}>
                  测试连接
                </button>
                <button className="proxy-save-btn" onClick={saveProxySettings}>
                  保存设置
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      <footer className="footer">
        <p>PWA Container v0.1.0 - 最多 {MAX_IFRAMES} 个后台应用</p>
      </footer>
    </div>
  );
}

export default App;

