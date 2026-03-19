import { useState, useEffect } from "react";
import { appDataDir } from "@tauri-apps/api/path";
import { readDir, remove, stat } from "@tauri-apps/plugin-fs";
import type { AppInfo } from "./types";
import { confirmDialog } from "./ConfirmDialog";
import "./AppDataManager.css";

interface AppDataUsage {
  total_bytes: number;
  file_count: number;
}

interface AppDataManagerProps {
  app: AppInfo | null;
  show: boolean;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

// 递归计算目录大小
async function calcDirSize(dirPath: string): Promise<number> {
  let total = 0;
  try {
    const entries = await readDir(dirPath);
    for (const entry of entries) {
      if (entry.isDirectory) {
        total += await calcDirSize(`${dirPath}/${entry.name}`);
      } else if (entry.isFile) {
        try {
          const info = await stat(`${dirPath}/${entry.name}`);
          total += info.size;
        } catch {
          // 忽略无法读取的文件
        }
      }
    }
  } catch {
    // 目录不存在或无法读取
  }
  return total;
}

// 递归计算文件数
async function calcFileCount(dirPath: string): Promise<number> {
  let count = 0;
  try {
    const entries = await readDir(dirPath);
    for (const entry of entries) {
      if (entry.isDirectory) {
        count += await calcFileCount(`${dirPath}/${entry.name}`);
      } else if (entry.isFile) {
        count++;
      }
    }
  } catch {
    // 忽略错误
  }
  return count;
}

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
};

export function AppDataManager({ app, show, onClose, showMessage }: AppDataManagerProps) {
  const [dataUsage, setDataUsage] = useState<AppDataUsage | null>(null);
  const [loading, setLoading] = useState(false);
  const [formattedSize, setFormattedSize] = useState("-");

  const loadDataUsage = async () => {
    if (!app) return;
    
    setLoading(true);
    try {
      const appDataPath = await appDataDir();
      
      // 1. 计算 pwa_data 目录大小
      const appPath = `${appDataPath}/pwa_data/${app.id}`;
      const [dirSize, dirCount] = await Promise.all([
        calcDirSize(appPath),
        calcFileCount(appPath),
      ]);
      
      // 2. 计算 store 文件大小
      let storeSize = 0;
      let storeCount = 0;
      try {
        // localStorage 文件
        try {
          const lsStat = await stat(`${appDataPath}/pwa_data/stores/pwa-${app.id}.json`);
          storeSize += lsStat.size;
          storeCount++;
        } catch {
          // 文件不存在
        }
        
        // Cache 文件
        try {
          const cacheFiles = await readDir(`${appDataPath}/pwa_data/cache`);
          const appCacheFiles = cacheFiles.filter(
            entry => entry.isFile && entry.name?.startsWith(`pwa-${app.id}-cache-`) && entry.name?.endsWith('.json')
          );
          for (const file of appCacheFiles) {
            try {
              const fileStat = await stat(`${appDataPath}/pwa_data/cache/${file.name}`);
              storeSize += fileStat.size;
              storeCount++;
            } catch {
              // 忽略错误
            }
          }
        } catch {
          // cache 目录可能不存在
        }
      } catch {
        // 忽略错误
      }
      
      setDataUsage({ total_bytes: dirSize + storeSize, file_count: dirCount + storeCount });
      setFormattedSize(formatBytes(dirSize + storeSize));
    } catch (error) {
      showMessage("error", `加载数据信息失败: ${error}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (show && app) {
      loadDataUsage();
    }
  }, [show, app]);

  const handleClearData = async () => {
    if (!app) return;

    const confirmed = await confirmDialog({
      title: "清理应用数据",
      message: `确定要清理 "${app.name}" 的所有数据吗？\n此操作不可恢复。`,
      isDanger: true,
    });
    if (!confirmed) return;

    try {
      const appDataPath = await appDataDir();
      
      // 1. 清理 pwa_data 目录
      const appPath = `${appDataPath}/pwa_data/${app.id}`;
      try {
        await remove(appPath, { recursive: true });
      } catch {
        // 目录可能不存在
      }
      
      // 2. 清理 localStorage store 文件
      try {
        await remove(`${appDataPath}/pwa_data/stores/pwa-${app.id}.json`);
      } catch {
        // 文件可能不存在
      }
      
      // 3. 清理 Cache API 的 store 文件
      try {
        const cacheDir = `${appDataPath}/pwa_data/cache`;
        const cacheFiles = await readDir(cacheDir);
        const appCacheFiles = cacheFiles.filter(
          entry => entry.isFile && entry.name?.startsWith(`pwa-${app.id}-cache-`) && entry.name?.endsWith('.json')
        );
        for (const file of appCacheFiles) {
          await remove(`${cacheDir}/${file.name}`);
        }
      } catch {
        // cache 目录可能不存在
      }
      
      showMessage("success", `已清理 ${app.name} 的所有数据`);
      loadDataUsage();
    } catch (error) {
      showMessage("error", `清理数据失败: ${error}`);
    }
  };

  if (!show || !app) return null;

  return (
    <div className="app-data-overlay" onClick={onClose}>
      <div className="app-data-modal" onClick={(e) => e.stopPropagation()}>
        <div className="app-data-header">
          <h3>🗑️ {app.name} - 数据管理</h3>
          <button className="close-btn" onClick={onClose}>✕</button>
        </div>

        <div className="app-data-content">
          {loading ? (
            <div className="loading">加载中...</div>
          ) : dataUsage ? (
            <>
              <div className="data-summary">
                <div className="total-size">{formattedSize}</div>
                <div className="total-label">总数据大小</div>
              </div>

              <div className="data-details">
                <h4>数据详情</h4>
                <div className="detail-list">
                  <div className="detail-item">
                    <span className="detail-name">🗄️ SQLite 数据库</span>
                    <span className="detail-desc">SQL 查询数据</span>
                  </div>
                  <div className="detail-item">
                    <span className="detail-name">💾 LocalStorage</span>
                    <span className="detail-desc">pwa-{app.id}.json</span>
                  </div>
                  <div className="detail-item">
                    <span className="detail-name">📦 Cache API</span>
                    <span className="detail-desc">pwa_data/cache/pwa-{app.id}-cache-*.json</span>
                  </div>
                  <div className="detail-item">
                    <span className="detail-name">📄 文件总数</span>
                    <span className="detail-size">{dataUsage.file_count} 个</span>
                  </div>
                </div>
              </div>

              <div className="data-actions">
                <h4>清理操作</h4>
                <div className="action-buttons">
                  <button 
                    className="action-btn danger"
                    onClick={handleClearData}
                  >
                    ⚠️ 清理所有数据
                  </button>
                </div>
                <p className="action-hint">
                  💡 提示：清理数据后，应用可能需要重新登录
                </p>
              </div>
            </>
          ) : (
            <div className="no-data">
              <p>暂无数据</p>
              <p className="no-data-hint">该应用尚未产生本地数据</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}