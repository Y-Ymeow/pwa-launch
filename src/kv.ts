/**
 * KV 存储前端实现
 * 使用主数据库 kv_store 表，与 appdata 协议统一
 */
import { appDataDir } from "@tauri-apps/api/path";

// 主数据库连接单例
let mainDb: any = null;
let mainDbPromise: Promise<any> | null = null;

// 获取主数据库连接（单例模式）
async function getMainDb() {
  if (mainDb) return mainDb;
  if (mainDbPromise) return mainDbPromise;

  mainDbPromise = (async () => {
    const { default: Database } = await import("@tauri-apps/plugin-sql");
    const appDataPath = await appDataDir();
    const dbPath = `sqlite:${appDataPath}/pwa_container.db`;
    mainDb = await Database.load(dbPath);
    return mainDb;
  })();

  return mainDbPromise;
}

// 关闭主数据库连接（应用退出时调用）
export async function closeMainDb() {
  if (mainDb) {
    await mainDb.close();
    mainDb = null;
    mainDbPromise = null;
  }
}

/**
 * 获取值
 * @param appId 应用 ID（如 'browser'）
 * @param key 键名
 * @returns 值或 null
 */
export async function kvGet(
  appId: string,
  key: string
): Promise<string | null> {
  const db = await getMainDb();
  const result = await db.select<{ value: string }[]>(
    "SELECT value FROM kv_store WHERE app_id = ? AND key = ?",
    [appId, key]
  );
  return result[0]?.value || null;
}

/**
 * 设置值
 * @param appId 应用 ID（如 'browser'）
 * @param key 键名
 * @param value 值
 */
export async function kvSet(
  appId: string,
  key: string,
  value: string
): Promise<void> {
  const db = await getMainDb();
  await db.execute(
    "INSERT OR REPLACE INTO kv_store (app_id, key, value) VALUES (?, ?, ?)",
    [appId, key, value]
  );
}

/**
 * 删除值
 * @param appId 应用 ID
 * @param key 键名
 */
export async function kvRemove(appId: string, key: string): Promise<void> {
  const db = await getMainDb();
  await db.execute(
    "DELETE FROM kv_store WHERE app_id = ? AND key = ?",
    [appId, key]
  );
}

/**
 * 获取所有键值对
 * @param appId 应用 ID
 * @returns 键值对对象
 */
export async function kvGetAll(
  appId: string
): Promise<Record<string, string>> {
  const db = await getMainDb();
  const result = await db.select<{ key: string; value: string }[]>(
    "SELECT key, value FROM kv_store WHERE app_id = ?",
    [appId]
  );
  const map: Record<string, string> = {};
  for (const row of result) {
    map[row.key] = row.value;
  }
  return map;
}

/**
 * 清空所有值
 * @param appId 应用 ID（不传则清空所有）
 */
export async function kvClear(appId?: string): Promise<void> {
  const db = await getMainDb();
  if (appId) {
    await db.execute("DELETE FROM kv_store WHERE app_id = ?", [appId]);
  } else {
    await db.execute("DELETE FROM kv_store");
  }
}
