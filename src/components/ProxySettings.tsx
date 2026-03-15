import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import type { ProxySettings as ProxySettingsType, CommandResponse } from './types';

interface ProxySettingsProps {
  show: boolean;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

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
    // 检查是否在 Tauri 环境中
    if (typeof window.__TAURI_INTERNALS__ === 'undefined') {
      console.log('不在 Tauri 环境中，跳过加载代理设置');
      return;
    }
    try {
      const response = await invoke<CommandResponse<ProxySettingsType | null>>("get_proxy");
      if (response.success && response.data) {
        setSettings({
          ...response.data,
          username: response.data.username || "",
          password: response.data.password || "",
        });
      }
    } catch (error) {
      console.error("加载代理设置失败:", error);
    }
  };

  const saveSettings = async () => {
    // 检查是否在 Tauri 环境中
    if (typeof window.__TAURI_INTERNALS__ === 'undefined') {
      showMessage("error", "请在 Tauri 应用中保存设置");
      return;
    }
    try {
      await invoke("set_proxy", {
        enabled: settings.enabled,
        proxyType: settings.proxy_type,
        host: settings.host,
        port: settings.port,
        username: settings.username || null,
        password: settings.password || null,
      });
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
    try {
      await invoke("set_proxy", {
        enabled: settings.enabled,
        proxyType: settings.proxy_type,
        host: settings.host,
        port: settings.port,
        username: settings.username || null,
        password: settings.password || null,
      });

      const response = await invoke<CommandResponse<{ status: number }>>("proxy_fetch", {
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
