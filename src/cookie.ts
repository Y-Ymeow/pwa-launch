/**
 * Cookie 管理前端实现
 * 使用 SQL 直接操作数据库
 */
import { appDataDir } from "@tauri-apps/api/path";

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
export async function closeCookieDb() {
  if (dbInstance) {
    await dbInstance.close();
    dbInstance = null;
    dbPromise = null;
  }
}

// 提取域名
function extractDomain(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

// Cookie 对象
export interface Cookie {
  name: string;
  value: string;
  domain: string;
  path?: string;
  expires?: number;
  secure?: boolean;
  httpOnly?: boolean;
}

// 获取指定域名的 cookies
export async function getCookies(
  url: string,
  appId: string
): Promise<Record<string, string>> {
  const domain = extractDomain(url);
  const db = await getDb();

  const result = await db.select<{ name: string; value: string }[]>(
    "SELECT name, value FROM cookies WHERE app_id = ? AND domain = ?",
    [appId, domain]
  );

  const cookies: Record<string, string> = {};
  for (const row of result) {
    cookies[row.name] = row.value;
  }
  return cookies;
}

// 设置 cookies
export async function setCookies(
  url: string,
  appId: string,
  cookies: string[]
): Promise<void> {
  const domain = extractDomain(url);
  const db = await getDb();

  for (const cookie of cookies) {
    const eqPos = cookie.indexOf("=");
    if (eqPos > 0) {
      const name = cookie.substring(0, eqPos).trim();
      const value = cookie.substring(eqPos + 1).trim();
      if (name && value) {
        await db.execute(
          `INSERT OR REPLACE INTO cookies 
           (app_id, domain, name, value, updated_at) 
           VALUES (?, ?, ?, ?, strftime('%s', 'now'))`,
          [appId, domain, name, value]
        );
      }
    }
  }
}

// 清除 cookies
export async function clearCookies(
  appId: string,
  domain?: string,
  includeSubdomains: boolean = true
): Promise<void> {
  const db = await getDb();

  if (domain) {
    if (includeSubdomains) {
      // 清除域名及其子域
      const likePattern = `%.${domain}`;
      await db.execute(
        `DELETE FROM cookies 
         WHERE app_id = ? AND (domain = ? OR domain LIKE ?)`,
        [appId, domain, likePattern]
      );
    } else {
      await db.execute(
        "DELETE FROM cookies WHERE app_id = ? AND domain = ?",
        [appId, domain]
      );
    }
  } else {
    // 清除所有 cookies
    await db.execute("DELETE FROM cookies WHERE app_id = ?", [appId]);
  }
}

// 获取所有 cookie 域名
export async function getCookieDomains(): Promise<string[]> {
  const db = await getDb();
  const result = await db.select<{ domain: string }[]>(
    "SELECT DISTINCT domain FROM cookies ORDER BY domain"
  );
  return result.map((r) => r.domain);
}

// 解析并保存 cookie 字符串（格式: "key1=value1; key2=value2"）
export async function parseAndSaveCookies(
  cookieString: string,
  appId: string,
  domain: string
): Promise<number> {
  const db = await getDb();
  let count = 0;

  for (const cookie of cookieString.split(";")) {
    const trimmed = cookie.trim();
    const eqPos = trimmed.indexOf("=");
    if (eqPos > 0) {
      const name = trimmed.substring(0, eqPos).trim();
      const value = trimmed.substring(eqPos + 1).trim();
      if (name) {
        await db.execute(
          `INSERT OR REPLACE INTO cookies 
           (app_id, domain, name, value, updated_at) 
           VALUES (?, ?, ?, ?, strftime('%s', 'now'))`,
          [appId, domain, name, value]
        );
        count++;
      }
    }
  }
  return count;
}

// 同步 WebView cookies（从 document.cookie 同步到数据库）
export async function syncWebviewCookies(
  domain: string,
  cookieString: string,
  userAgent?: string
): Promise<number> {
  console.log("[Cookies] Syncing for domain:", domain, "UA:", userAgent);
  return await parseAndSaveCookies(cookieString, "webview", domain);
}

// 获取所有 cookies（不分应用）
export async function getAllCookies(
  appId: string
): Promise<Record<string, string>> {
  const db = await getDb();
  const result = await db.select<{ name: string; value: string }[]>(
    "SELECT name, value FROM cookies WHERE app_id = ?",
    [appId]
  );

  const cookies: Record<string, string> = {};
  for (const row of result) {
    cookies[row.name] = row.value;
  }
  return cookies;
}
