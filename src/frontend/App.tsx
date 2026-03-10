import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './styles/App.css';

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

function App() {
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [installUrl, setInstallUrl] = useState('');
  const [installing, setInstalling] = useState(false);
  const [selectedApp, setSelectedApp] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<{ type: 'success' | 'error', text: string } | null>(null);
  const [runningWindows, setRunningWindows] = useState<string[]>([]);

  // 加载应用列表
  const loadApps = async () => {
    try {
      const response = await invoke<CommandResponse<AppInfo[]>>('list_apps');
      if (response.success && response.data) {
        setApps(response.data);
      }
    } catch (error) {
      showMessage('error', `加载应用列表失败：${error}`);
    } finally {
      setLoading(false);
    }
  };

  // 加载运行中的窗口
  const loadRunningWindows = async () => {
    try {
      const response = await invoke<CommandResponse<string[]>>('list_running_pwas');
      if (response.success && response.data) {
        setRunningWindows(response.data);
      }
    } catch (error) {
      console.error('加载运行窗口失败:', error);
    }
  };

  useEffect(() => {
    loadApps();
    loadRunningWindows();
    // 每 5 秒刷新一次运行状态
    const interval = setInterval(loadRunningWindows, 5000);
    return () => clearInterval(interval);
  }, []);

  // 安装 PWA
  const handleInstall = async (e: React.FormEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!installUrl.trim()) return;

    setInstalling(true);
    try {
      const response = await invoke<CommandResponse<AppInfo>>('install_pwa', {
        request: { url: installUrl.trim() }
      });

      if (response.success && response.data) {
        showMessage('success', `应用 "${response.data.name}" 安装成功！`);
        setInstallUrl('');
        loadApps();
      } else {
        showMessage('error', response.error || '安装失败');
      }
    } catch (error) {
      showMessage('error', `安装失败：${error}`);
    } finally {
      setInstalling(false);
    }
  };

  // 启动应用
  const handleLaunch = async (app: AppInfo) => {
    try {
      const response = await invoke<CommandResponse<string>>('launch_app', { appId: app.id });
      if (response.success && response.data) {
        showMessage('success', `${app.name} 已启动！`);
        loadRunningWindows();
      }
    } catch (error) {
      showMessage('error', `启动失败：${error}`);
    }
  };

  // 关闭应用窗口
  const handleCloseWindow = async (windowId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const response = await invoke<CommandResponse<boolean>>('close_pwa_window', { windowId });
      if (response.success && response.data) {
        loadRunningWindows();
      }
    } catch (error) {
      showMessage('error', `关闭失败：${error}`);
    }
  };

  // 卸载应用
  const handleUninstall = async (appId: string) => {
    if (!confirm('确定要卸载这个应用吗？')) return;

    try {
      const response = await invoke<CommandResponse<boolean>>('uninstall_pwa', { appId });
      if (response.success && response.data) {
        showMessage('success', '应用已卸载');
        loadApps();
        if (selectedApp?.id === appId) {
          setSelectedApp(null);
        }
      }
    } catch (error) {
      showMessage('error', `卸载失败：${error}`);
    }
  };

  // 清除数据
  const handleClearData = async (appId: string) => {
    if (!confirm('确定要清除这个应用的所有数据吗？')) return;

    try {
      const response = await invoke<CommandResponse<number>>('clear_data', { appId });
      if (response.success && response.data) {
        const size = (response.data / 1024).toFixed(2);
        showMessage('success', `已清除 ${size} KB 数据`);
      }
    } catch (error) {
      showMessage('error', `清除数据失败：${error}`);
    }
  };

  // 备份数据
  const handleBackup = async (appId: string) => {
    try {
      const response = await invoke<CommandResponse<any>>('backup_data', { appId });
      if (response.success && response.data) {
        showMessage('success', `备份完成！路径：${response.data.backup_path}`);
      }
    } catch (error) {
      showMessage('error', `备份失败：${error}`);
    }
  };

  // 创建快捷方式
  const handleCreateShortcut = async (appId: string) => {
    try {
      const response = await invoke<CommandResponse<any>>('create_shortcut', { appId });
      if (response.success && response.data) {
        showMessage('success', `快捷方式已创建！路径：${response.data.shortcut_path}`);
      }
    } catch (error) {
      showMessage('error', `创建快捷方式失败：${error}`);
    }
  };

  const showMessage = (type: 'success' | 'error', text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString('zh-CN');
  };

  return (
    <div className="app">
      <header className="header">
        <h1>🚀 PWA Container</h1>
        <p className="subtitle">跨平台 PWA 应用容器</p>
      </header>

      {/* 消息提示 */}
      {message && (
        <div className={`message ${message.type}`}>
          {message.text}
        </div>
      )}

      <main className="main">
        {/* 运行状态 */}
        {runningWindows.length > 0 && (
          <section className="running-section">
            <h2>🔴 运行中 ({runningWindows.length})</h2>
            <div className="running-list">
              {runningWindows.map(windowId => (
                <div key={windowId} className="running-item">
                  <span className="running-indicator">●</span>
                  <span className="window-id">{windowId}</span>
                  <button 
                    className="btn-close"
                    onClick={(e) => handleCloseWindow(windowId, e)}
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          </section>
        )}

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
              {installing ? '安装中...' : '安装应用'}
            </button>
          </form>
        </section>

        {/* 应用列表 */}
        <section className="apps-section">
          <h2>已安装的应用 ({apps.length})</h2>
          
          {loading ? (
            <div className="loading">加载中...</div>
          ) : apps.length === 0 ? (
            <div className="empty-state">
              <p>暂无应用，安装一个 PWA 应用开始使用吧！</p>
            </div>
          ) : (
            <div className="apps-grid">
              {apps.map((app) => (
                <div
                  key={app.id}
                  className={`app-card ${selectedApp?.id === app.id ? 'selected' : ''}`}
                  onClick={() => setSelectedApp(app)}
                >
                  <div className="app-icon">
                    {app.icon_url ? (
                      <img src={app.icon_url} alt={app.name} onError={(e) => {
                        (e.target as HTMLImageElement).src = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><text y="50" font-size="50">📱</text></svg>';
                      }} />
                    ) : (
                      <span>📱</span>
                    )}
                  </div>
                  <h3>{app.name}</h3>
                  <p className="app-url" title={app.url}>{app.url}</p>
                  <p className="app-date">安装于：{formatDate(app.installed_at)}</p>
                  
                  <div className="app-actions">
                    <button 
                      className="btn-launch"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleLaunch(app);
                      }}
                    >
                      🚀 启动
                    </button>
                    <button 
                      className="btn-action"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleCreateShortcut(app.id);
                      }}
                    >
                      🔗 快捷方式
                    </button>
                    <button 
                      className="btn-action"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleBackup(app.id);
                      }}
                    >
                      💾 备份
                    </button>
                    <button 
                      className="btn-danger"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleClearData(app.id);
                      }}
                    >
                      🗑️ 清除数据
                    </button>
                    <button 
                      className="btn-uninstall"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleUninstall(app.id);
                      }}
                    >
                      ❌ 卸载
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* 应用详情 */}
        {selectedApp && (
          <section className="app-detail">
            <h2>应用详情</h2>
            <div className="detail-content">
              <div className="detail-row">
                <strong>名称:</strong> {selectedApp.name}
              </div>
              <div className="detail-row">
                <strong>ID:</strong> {selectedApp.id}
              </div>
              <div className="detail-row">
                <strong>URL:</strong> <a href={selectedApp.url} target="_blank" rel="noopener noreferrer">{selectedApp.url}</a>
              </div>
              <div className="detail-row">
                <strong>显示模式:</strong> {selectedApp.display_mode}
              </div>
              <div className="detail-row">
                <strong>安装日期:</strong> {formatDate(selectedApp.installed_at)}
              </div>
            </div>
          </section>
        )}
      </main>

      <footer className="footer">
        <p>PWA Container v0.1.0 - 跨平台 PWA 应用容器</p>
      </footer>
    </div>
  );
}

export default App;
