import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir, appCacheDir } from "@tauri-apps/api/path";
import { remove } from "@tauri-apps/plugin-fs";
import { confirmDialog } from "./ConfirmDialog";
import { kvClear } from "../kv";
import "./DataManager.css";

interface DataManagerProps {
  show: boolean;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

export function DataManager({ show, onClose, showMessage }: DataManagerProps) {
  const [clearing, setClearing] = useState<string | null>(null);

  const handleClearMainDb = async () => {
    const confirmed = await confirmDialog({
      title: "清理主数据库",
      message: "确定要清理主数据库吗？这将删除所有应用列表信息，但不会影响已安装应用的数据。",
      isDanger: true,
    });
    if (!confirmed) return;

    setClearing("db");
    try {
      const appDataPath = await appDataDir();
      await remove(`${appDataPath}/pwa_container.db`);
      showMessage("success", "主数据库已清理");
    } catch (error) {
      showMessage("error", `清理失败: ${error}`);
    } finally {
      setClearing(null);
    }
  };

  const handleClearStore = async () => {
    const confirmed = await confirmDialog({
      title: "清理配置数据",
      message: "确定要清理所有配置数据吗？这将重置代理设置、浏览器历史等。",
      isDanger: true,
    });
    if (!confirmed) return;

    setClearing("store");
    try {
      // 清理代理配置
      await kvClear("proxy");
      // 清理浏览器数据
      await kvClear("browser");

      showMessage("success", "配置数据已清理");
    } catch (error) {
      showMessage("error", `清理失败: ${error}`);
    } finally {
      setClearing(null);
    }
  };

  const handleClearHttpCache = async () => {
    const confirmed = await confirmDialog({
      title: "清理 HTTP 缓存",
      message: "确定要清理 HTTP 缓存吗？",
      isDanger: true,
    });
    if (!confirmed) return;

    setClearing("http");
    try {
      const cachePath = await appCacheDir();
      await remove(cachePath, { recursive: true });
      showMessage("success", "HTTP 缓存已清理");
    } catch (error) {
      showMessage("error", `清理失败: ${error}`);
    } finally {
      setClearing(null);
    }
  };

  const handleClearWebviewCache = async () => {
    const confirmed = await confirmDialog({
      title: "清理 WebView 缓存",
      message: "确定要清理 WebView 缓存吗？",
      isDanger: true,
    });
    if (!confirmed) return;

    setClearing("webview");
    try {
      await invoke("clear_webview_cache_data");
      showMessage("success", "WebView 缓存已清理");
    } catch (error) {
      showMessage("error", `清理失败: ${error}`);
    } finally {
      setClearing(null);
    }
  };

  if (!show) return null;

  return (
    <div className="data-manager-overlay" onClick={onClose}>
      <div className="data-manager-modal" onClick={(e) => e.stopPropagation()}>
        <div className="data-manager-header">
          <h2>数据清理</h2>
          <button className="close-btn" onClick={onClose}>✕</button>
        </div>

        <div className="data-manager-content">
          <div className="cleanup-grid">
            <div className="cleanup-card" onClick={handleClearMainDb}>
              <div className="cleanup-icon">🗄️</div>
              <h3>主数据库</h3>
              <p>清理应用列表数据库</p>
              <button disabled={clearing === "db"}>
                {clearing === "db" ? "清理中..." : "清理"}
              </button>
            </div>

            <div className="cleanup-card" onClick={handleClearStore}>
              <div className="cleanup-icon">⚙️</div>
              <h3>配置数据</h3>
              <p>清理代理设置、历史记录等</p>
              <button disabled={clearing === "store"}>
                {clearing === "store" ? "清理中..." : "清理"}
              </button>
            </div>

            <div className="cleanup-card" onClick={handleClearHttpCache}>
              <div className="cleanup-icon">📦</div>
              <h3>HTTP 缓存</h3>
              <p>清理网络请求缓存</p>
              <button disabled={clearing === "http"}>
                {clearing === "http" ? "清理中..." : "清理"}
              </button>
            </div>

            <div className="cleanup-card" onClick={handleClearWebviewCache}>
              <div className="cleanup-icon">🌐</div>
              <h3>WebView 缓存</h3>
              <p>清理浏览器缓存数据</p>
              <button disabled={clearing === "webview"}>
                {clearing === "webview" ? "清理中..." : "清理"}
              </button>
            </div>
          </div>

          <div className="data-hint">
            💡 提示：各 PWA 的应用数据需要到对应应用的管理页面清理
          </div>
        </div>
      </div>
    </div>
  );
}
