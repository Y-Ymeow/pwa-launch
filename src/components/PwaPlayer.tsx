import { useRef, memo } from 'react';
import type { RunningPwa, PwaSnapshot, AppInfo } from './types';

interface PwaPlayerProps {
  runningPwas: RunningPwa[];
  activePwaId: string | null;
  restoringPwa: string | null;
  snapshots: Record<string, PwaSnapshot>;
  apps: AppInfo[];
  getProxiedUrl: (url: string) => string;
  handleIframeLoad: (appId: string) => void;
  setActivePwaId: (id: string | null) => void;
  setViewMode: (mode: 'apps' | 'browser' | 'pwa') => void;
  setRunningPwas: React.Dispatch<React.SetStateAction<RunningPwa[]>>;
  setShowSwitcher: (show: boolean) => void;
  closePwa: (appId: string) => void;
  refreshPwa: (appId: string) => void;
  getAppIcon: (appId: string) => string | undefined;
}

function PwaPlayerComponent({
  runningPwas,
  activePwaId,
  restoringPwa,
  snapshots,
  apps,
  getProxiedUrl,
  handleIframeLoad,
  setActivePwaId,
  setViewMode,
  setRunningPwas,
  setShowSwitcher,
  closePwa,
  refreshPwa,
  getAppIcon,
}: PwaPlayerProps) {
  const iframesRef = useRef<Record<string, HTMLIFrameElement>>({});

  return (
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

      {/* 悬浮切换面板 */}
      <div className="floating-switcher right">
        <button
          className="fab"
          onClick={() => setShowSwitcher(true)}
        >
          <span>{runningPwas.length}</span>
        </button>

        {/* 运行中的应用列表 */}
        <div className="switcher-panel">
          <div className="panel-header">
            <span>运行中的应用</span>
            <button
              className="btn-manage"
              onClick={() => {
                setViewMode('apps');
                setActivePwaId(null);
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
                    title="刷新页面"
                  >
                    ↻
                  </button>
                  <button
                    className="btn-close-item"
                    onClick={(e) => {
                      e.stopPropagation();
                      closePwa(pwa.appId);
                    }}
                    title="关闭应用"
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}

            {/* 已暂停的应用 */}
            {Object.values(snapshots).map((snapshot) => (
              <div
                key={snapshot.appId}
                className="running-item snapshot"
                onClick={() => {
                  const app = apps.find((a) => a.id === snapshot.appId);
                  if (app) {
                    // 这里需要调用 launchOrSwitchPwa，通过 props 传递
                  }
                }}
              >
                <div className="item-icon">
                  {getAppIcon(snapshot.appId) ? (
                    <img src={getAppIcon(snapshot.appId)} alt={snapshot.name} />
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
      </div>
    </div>
  );
}

export const PwaPlayer = memo(PwaPlayerComponent);
