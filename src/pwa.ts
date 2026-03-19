/**
 * PWA 管理前端实现
 * 使用 SQL 直接操作数据库
 */
import { appDataDir } from "@tauri-apps/api/path";
import type { AppInfo } from "./components/types";

// 数据库连接单例
let dbInstance: any = null;
let dbPromise: Promise<any> | null = null;

// 获取数据库连接（单例模式）
async function getDb() {
  if (dbInstance) return dbInstance;
  if (dbPromise) return dbPromise;

  dbPromise = (async () => {
    const { default: Database } = await import("@tauri-apps/plugin-sql");
    const appDataPath = await appDataDir();
    const dbPath = `sqlite:${appDataPath}/pwa_container.db`;
    dbInstance = await Database.load(dbPath);
    return dbInstance;
  })();

  return dbPromise;
}

// 关闭数据库连接（应用退出时调用）
export async function closePwaDb() {
  if (dbInstance) {
    await dbInstance.close();
    dbInstance = null;
    dbPromise = null;
  }
}

// 生成应用 ID
function generateAppId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).substr(2, 9)}`;
}

// 获取当前时间戳
function nowTimestamp(): number {
  return Math.floor(Date.now() / 1000);
}

// Manifest 信息
interface ManifestInfo {
  name?: string;
  icon_url?: string;
  manifest_url?: string;
  start_url?: string;
  scope?: string;
  theme_color?: string;
  background_color?: string;
  display_mode?: string;
}

// 解析 manifest
async function fetchManifestInfo(url: string): Promise<ManifestInfo> {
  const response = await fetch(url, {
    headers: {
      "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    },
  });
  const html = await response.text();

  // 提取 manifest 链接
  const manifestMatch = html.match(
    /<link\s+rel=["']manifest["']\s+href=["']([^"']+)["']/i
  );
  const manifestUrl = manifestMatch
    ? new URL(manifestMatch[1], url).href
    : undefined;

  // 提取标题
  const titleMatch = html.match(/<title>(.*?)<\/title>/i);
  const htmlTitle = titleMatch?.[1];

  // 提取图标
  const iconMatch = html.match(
    /<link\s+rel=["'](?:icon|shortcut icon|apple-touch-icon)["']\s+href=["']([^"']+)["']/i
  );
  const htmlIcon = iconMatch ? new URL(iconMatch[1], url).href : undefined;

  let info: ManifestInfo = {
    name: htmlTitle,
    icon_url: htmlIcon,
    manifest_url: manifestUrl,
    start_url: url,
    scope: undefined,
    theme_color: undefined,
    background_color: undefined,
    display_mode: "standalone",
  };

  // 如果存在 manifest，解析它
  if (manifestUrl) {
    try {
      const manifestRes = await fetch(manifestUrl);
      const manifest = await manifestRes.json();

      if (manifest.name || manifest.short_name) {
        info.name = manifest.name || manifest.short_name;
      }

      // 解析图标
      if (manifest.icons?.length > 0) {
        let bestIcon = manifest.icons[0];
        for (const icon of manifest.icons) {
          const size = parseInt(icon.sizes?.split("x")[0] || "0");
          const bestSize = parseInt(bestIcon.sizes?.split("x")[0] || "0");
          if (size > bestSize) {
            bestIcon = icon;
          }
        }
        info.icon_url = new URL(bestIcon.src, manifestUrl).href;
      }

      info.display_mode = manifest.display || "standalone";
      info.theme_color = manifest.theme_color;
      info.background_color = manifest.background_color;
      if (manifest.start_url) {
        info.start_url = new URL(manifest.start_url, url).href;
      }
    } catch (e) {
      console.error("[PWA] Failed to parse manifest:", e);
    }
  }

  return info;
}

// 安装 PWA
export async function installPwa(url: string, name?: string): Promise<AppInfo> {
  const db = await getDb();

  // 获取 manifest 信息
  const manifest = await fetchManifestInfo(url);

  const appId = generateAppId();
  const now = nowTimestamp();

  const appInfo: AppInfo = {
    id: appId,
    name: name || manifest.name || "未知应用",
    url,
    icon_url: manifest.icon_url,
    manifest_url: manifest.manifest_url,
    installed_at: now,
    updated_at: now,
    start_url: manifest.start_url || url,
    scope: manifest.scope,
    theme_color: manifest.theme_color,
    background_color: manifest.background_color,
    display_mode: manifest.display_mode || "standalone",
  };

  // 插入数据库
  await db.execute(
    `INSERT OR REPLACE INTO apps 
     (id, name, url, icon_url, manifest_url, installed_at, updated_at, start_url, scope, theme_color, background_color, display_mode)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      appInfo.id,
      appInfo.name,
      appInfo.url,
      appInfo.icon_url || "",
      appInfo.manifest_url || "",
      appInfo.installed_at,
      appInfo.updated_at,
      appInfo.start_url || "",
      appInfo.scope || "",
      appInfo.theme_color || "",
      appInfo.background_color || "",
      appInfo.display_mode,
    ]
  );

  console.log("[PWA] Installed:", appInfo);
  return appInfo;
}

// 卸载 PWA
export async function uninstallPwa(appId: string): Promise<void> {
  const db = await getDb();

  // 删除 apps 表记录
  await db.execute("DELETE FROM apps WHERE id = ?", [appId]);

  // 删除该 PWA 的 EAV 数据
  await db.execute("DELETE FROM pwa_data WHERE pwa_id = ?", [appId]);

  console.log("[PWA] Uninstalled:", appId);
}

// 获取应用列表
export async function listApps(): Promise<AppInfo[]> {
  const db = await getDb();
  const result = await db.select<AppInfo[]>(
    "SELECT * FROM apps ORDER BY installed_at DESC"
  );
  return result;
}

// 获取运行中的 PWA 列表（前端直接获取）
export function listRunningPwas(): string[] {
  // 从全局状态获取，由 App.tsx 维护
  if ((window as any).__RUNNING_PWAS__) {
    return (window as any).__RUNNING_PWAS__;
  }
  return [];
}

// 设置运行中的 PWA 列表（由 App.tsx 调用）
export function setRunningPwas(pwaIds: string[]) {
  (window as any).__RUNNING_PWAS__ = pwaIds;
}

// 获取单个应用信息
export async function getAppInfo(appId: string): Promise<AppInfo | null> {
  const db = await getDb();
  const result = await db.select<AppInfo[]>(
    "SELECT * FROM apps WHERE id = ?",
    [appId]
  );
  return result[0] || null;
}
