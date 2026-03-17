import { useRef, memo, useEffect, useState } from "react";
import type { RunningPwa, PwaSnapshot, AppInfo } from "./types";

interface PwaPlayerProps {
  runningPwas: RunningPwa[];
  activePwaId: string | null;
  restoringPwa: string | null;
  snapshots: Record<string, PwaSnapshot>;
  apps: AppInfo[];
  getProxiedUrl: (url: string) => string;
  handleIframeLoad: (appId: string) => void;
  setActivePwaId: (id: string | null) => void;
  setViewMode: (mode: "apps" | "browser" | "pwa") => void;
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
  // 存储 HTML 内容（key: appId, value: htmlContent）
  const [htmlContents, setHtmlContents] = useState<Record<string, string>>({});

  // 预加载 HTML 内容
  useEffect(() => {
    runningPwas.forEach(async (pwa) => {
      // 如果已经有缓存内容，跳过
      if (htmlContents[pwa.appId]) return;

      try {
        // 1. 先尝试从 persistent_cache 读取
        const cached = await loadHtmlFromCache(pwa.appId, pwa.url);
        if (cached) {
          setHtmlContents(prev => ({ ...prev, [pwa.appId]: cached }));
          return;
        }

        // 2. 缓存未命中，fetch 并缓存
        const html = await fetchAndCacheHtml(pwa.appId, pwa.url);
        if (html) {
          setHtmlContents(prev => ({ ...prev, [pwa.appId]: html }));
        }
      } catch (e) {
        console.error('[PwaPlayer] Failed to load HTML:', e);
      }
    });
  }, [runningPwas]);

  // 从缓存读取 HTML
  async function loadHtmlFromCache(appId: string, url: string): Promise<string | null> {
    try {
      const cacheKey = `html:${url}`;
      const result = await (window as any).__TAURI__?.persistentCache?.getItem(cacheKey);
      if (result && result.data) {
        console.log('[PwaPlayer] HTML cache hit:', url);
        // base64 解码
        return atob(result.data);
      }
    } catch (e) {
      console.error('[PwaPlayer] Failed to load from cache:', e);
    }
    return null;
  }

  // 获取并缓存 HTML
  async function fetchAndCacheHtml(appId: string, url: string): Promise<string | null> {
    try {
      console.log('[PwaPlayer] Fetching HTML:', url);
      const response = await fetch(url);
      if (!response.ok) return null;

      let html = await response.text();

      // 注入 adapt.js（确保 PWA 能调用 Tauri API）
      const adaptScript = `<script src="adapt.min.js"></script>`;
      if (html.includes('</head>')) {
        html = html.replace('</head>', `${adaptScript}</head>`);
      } else if (html.includes('<body')) {
        html = html.replace('<body', `${adaptScript}<body`);
      } else {
        html = adaptScript + html;
      }

      // 添加 <base> 标签确保相对路径正确
      const baseTag = `<base href="${url}">`;
      if (html.includes('<head')) {
        html = html.replace(/<head[^>]*>/, `$&${baseTag}`);
      } else {
        html = `<head>${baseTag}</head>` + html;
      }

      // 缓存到 persistent_cache
      const cacheKey = `html:${url}`;
      await (window as any).__TAURI__?.persistentCache?.setItem(cacheKey, html, {
        mimeType: 'text/html'
      });

      console.log('[PwaPlayer] HTML cached:', url);
      return html;
    } catch (e) {
      console.error('[PwaPlayer] Failed to fetch HTML:', e);
      return null;
    }
  }

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
            // 如果有缓存 HTML 用 srcdoc，否则用 src
            srcDoc={htmlContents[pwa.appId]}
            src={htmlContents[pwa.appId] ? undefined : getProxiedUrl(pwa.url)}
            sandbox="allow-scripts allow-same-origin allow-popups allow-forms allow-downloads allow-modals"
            allow="fullscreen; clipboard-write; autoplay"
            onLoad={() => handleIframeLoad(pwa.appId)}
            title={pwa.name}
          />
        </div>
      ))}

      {/* 悬浮切换面板 */}
      <div className="floating-switcher right">
        <button className="fab" onClick={() => setShowSwitcher(true)}>
          <span>{runningPwas.length}</span>
        </button>

        {/* 运行中的应用列表 */}
        <div className="switcher-panel">
          <div className="panel-header">
            <span>运行中的应用</span>
            <button
              className="btn-manage"
              onClick={() => {
                setViewMode("apps");
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
                  <div
                    id={"btn-refresh-item"}
                    onClick={(e) => {
                      e.stopPropagation();
                      refreshPwa(pwa.appId);
                    }}
                    title="刷新页面"
                  >
                    ↻
                  </div>
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
