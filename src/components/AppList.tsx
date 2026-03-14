import { useState, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  RunningPwa,
  PwaSnapshot,
  CommandResponse,
  ViewMode,
} from "./types";

interface AppListProps {
  apps: AppInfo[];
  runningPwas: RunningPwa[];
  snapshots: Record<string, PwaSnapshot>;
  loadApps: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
  setViewMode: (mode: ViewMode) => void;
  openBrowser: () => void;
  launchOrSwitchPwa: (app: AppInfo) => void;
  handleUninstall: (appId: string) => void;
}

export function AppList({
  apps,
  runningPwas,
  snapshots,
  loadApps,
  showMessage,
  setViewMode,
  openBrowser,
  launchOrSwitchPwa,
  handleUninstall,
}: AppListProps) {
  const [installUrl, setInstallUrl] = useState("");
  const [installing, setInstalling] = useState(false);

  const handleInstall = async (e: FormEvent) => {
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
      showMessage("error", `安装失败：${String(error)}`);
    } finally {
      setInstalling(false);
    }
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString("zh-CN");
  };

  return (
    <div className="management-area">
      <header className="header">
        <h1>🚀 PWA Container</h1>
        <p className="subtitle">应用管理 + 内置浏览器</p>
      </header>

      {/* 快捷入口 */}
      <section className="quick-actions">
        <button
          className="quick-btn browser-btn-large"
          onClick={() => openBrowser()}
        >
          <span className="quick-icon">🌐</span>
          <span className="quick-text">打开浏览器</span>
        </button>
      </section>

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
          <button type="submit" disabled={installing} className="btn-primary">
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
                          (e.target as HTMLImageElement).style.display = "none";
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
                      onClick={() => {
                        launchOrSwitchPwa(app);
                        setViewMode("pwa");
                      }}
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
  );
}
