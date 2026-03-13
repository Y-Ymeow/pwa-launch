import { useRef, useState } from 'react';
import { invoke } from "@tauri-apps/api/core";
import type { BrowserHistoryItem, CommandResponse, AppInfo } from './types';

interface BrowserViewProps {
  browserUrl: string;
  setBrowserUrl: (url: string) => void;
  browserHistory: BrowserHistoryItem[];
  setBrowserHistory: React.Dispatch<React.SetStateAction<BrowserHistoryItem[]>>;
  onClose: () => void;
  getProxiedUrl: (url: string) => string;
  showMessage: (type: "success" | "error", text: string) => void;
}

export function BrowserView({
  browserUrl,
  setBrowserUrl,
  browserHistory,
  setBrowserHistory,
  onClose,
  getProxiedUrl,
  showMessage,
}: BrowserViewProps) {
  const [showHistory, setShowHistory] = useState(false);
  const [installing, setInstalling] = useState(false);
  const browserIframeRef = useRef<HTMLIFrameElement>(null);

  const addToHistory = (url: string, title: string) => {
    setBrowserHistory((prev) => {
      const filtered = prev.filter((h) => h.url !== url);
      return [{ url, title, timestamp: Date.now() }, ...filtered].slice(0, 50);
    });
  };

  const browserNavigate = (url: string) => {
    let finalUrl = url;
    if (!url.startsWith('http')) {
      finalUrl = 'https://' + url;
    }
    setBrowserUrl(finalUrl);
    addToHistory(finalUrl, finalUrl);
    if (browserIframeRef.current) {
      browserIframeRef.current.src = getProxiedUrl(finalUrl);
    }
  };

  const browserRefresh = () => {
    if (browserIframeRef.current && browserUrl) {
      browserIframeRef.current.src = getProxiedUrl(browserUrl);
    }
  };

  const installFromBrowser = async () => {
    if (!browserUrl) return;

    setInstalling(true);
    try {
      const response = await invoke<CommandResponse<AppInfo>>("install_pwa", {
        request: { url: browserUrl },
      });

      if (response.success && response.data) {
        showMessage("success", `应用 "${response.data.name}" 安装成功！`);
      } else {
        showMessage("error", response.error || "安装失败");
      }
    } catch (error) {
      showMessage("error", `安装失败：${error}`);
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="browser-container">
      <div className="browser-toolbar">
        <button className="browser-btn" onClick={onClose} title="返回应用列表">
          ←
        </button>
        <button className="browser-btn" onClick={browserRefresh} title="刷新">
          ↻
        </button>
        <form
          className="browser-address-bar"
          onSubmit={(e) => {
            e.preventDefault();
            browserNavigate(browserUrl);
          }}
        >
          <input
            type="text"
            value={browserUrl}
            onChange={(e) => setBrowserUrl(e.target.value)}
            placeholder="输入网址..."
          />
        </form>
        <button
          className="browser-btn install-btn"
          onClick={installFromBrowser}
          disabled={!browserUrl || installing}
          title="将当前页面安装为应用"
        >
          {installing ? '...' : '⬇️'}
        </button>
        <button
          className="browser-btn"
          onClick={() => setShowHistory(!showHistory)}
          title="历史记录"
        >
          🕐
        </button>
      </div>

      {showHistory && (
        <div className="browser-history">
          {browserHistory.length === 0 ? (
            <div className="history-empty">暂无历史记录</div>
          ) : (
            browserHistory.map((item, idx) => (
              <div
                key={idx}
                className="history-item"
                onClick={() => {
                  browserNavigate(item.url);
                  setShowHistory(false);
                }}
              >
                <span className="history-title">{item.title}</span>
                <span className="history-url">{item.url}</span>
              </div>
            ))
          )}
        </div>
      )}

      {browserUrl ? (
        <iframe
          ref={browserIframeRef}
          src={getProxiedUrl(browserUrl)}
          sandbox="allow-scripts allow-same-origin allow-popups allow-forms allow-downloads allow-modals"
          allow="fullscreen; clipboard-write; autoplay"
          className="browser-iframe"
        />
      ) : (
        <div className="browser-welcome">
          <h2>🌐 内置浏览器</h2>
          <p>在地址栏输入网址开始浏览</p>
          <div className="browser-shortcuts">
            <button onClick={() => browserNavigate('https://www.bing.com')}>
              Bing
            </button>
            <button onClick={() => browserNavigate('https://www.baidu.com')}>
              百度
            </button>
            <button onClick={() => browserNavigate('https://github.com')}>
              GitHub
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
