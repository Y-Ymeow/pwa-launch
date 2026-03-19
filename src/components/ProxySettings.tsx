import { useState, useEffect } from 'react';
import type { ProxySettings as ProxySettingsType } from './types';
import { kvGet, kvSet } from "../kv";

interface ProxySettingsProps {
  show: boolean;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

const STORE_KEY = "proxy";

export function ProxySettings({ show, onClose, showMessage }: ProxySettingsProps) {
  const [settings, setSettings] = useState<ProxySettingsType>({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 8080,
    username: "",
    password: "",
  });

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      const proxyJson = await kvGet("proxy", "config");
      if (proxyJson) {
        const proxy = JSON.parse(proxyJson) as ProxySettingsType;
        setSettings({
          ...proxy,
          username: proxy.username || "",
          password: proxy.password || "",
        });
      }
    } catch (error) {
      console.error("加载代理设置失败:", error);
    }
  };

  const saveSettings = async () => {
    try {
      await kvSet("proxy", "config", JSON.stringify(settings));
      showMessage("success", "代理设置已保存");
      onClose();
    } catch (error) {
      showMessage("error", `保存代理设置失败：${error}`);
    }
  };

  const testProxy = async () => {
    // 检查是否在 Tauri 环境中
    if (typeof window.__TAURI_INTERNALS__ === 'undefined') {
      showMessage("error", "请在 Tauri 应用中测试代理");
      return;
    }
    showMessage("info", "代理测试功能暂未实现，请保存后使用");
  };

  if (!show) return null;

  return (
    <div className="proxy-settings-modal" onClick={onClose}>
      <div className="proxy-settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="proxy-settings-header">
          <h3>代理设置</h3>
          <button onClick={onClose}>✕</button>
        </div>
        <div className="proxy-settings-body">
          <label className="proxy-enable-label">
            <input
              type="checkbox"
              checked={settings.enabled}
              onChange={(e) => setSettings({ ...settings, enabled: e.target.checked })}
            />
            启用代理
          </label>

          <div className="proxy-field">
            <label>代理类型：</label>
            <select
              value={settings.proxy_type}
              onChange={(e) =>
                setSettings({ ...settings, proxy_type: e.target.value as "http" | "https" | "socks5" })
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
              value={settings.host}
              onChange={(e) => setSettings({ ...settings, host: e.target.value })}
            />
          </div>

          <div className="proxy-field">
            <label>端口：</label>
            <input
              type="number"
              placeholder="8080"
              value={settings.port}
              onChange={(e) => setSettings({ ...settings, port: parseInt(e.target.value) || 0 })}
            />
          </div>

          <div className="proxy-field">
            <label>用户名（可选）：</label>
            <input
              type="text"
              value={settings.username}
              onChange={(e) => setSettings({ ...settings, username: e.target.value })}
            />
          </div>

          <div className="proxy-field">
            <label>密码（可选）：</label>
            <input
              type="password"
              value={settings.password}
              onChange={(e) => setSettings({ ...settings, password: e.target.value })}
            />
          </div>

          <div className="proxy-actions">
            <button className="proxy-test-btn" onClick={testProxy}>
              测试连接
            </button>
            <button className="proxy-save-btn" onClick={saveSettings}>
              保存设置
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
