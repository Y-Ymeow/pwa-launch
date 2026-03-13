/**
 * Storage APIs (KV storage, localStorage hack)
 */

export function createStorage(bridge) {
  return {
    get _appId() {
      try {
        const contextCookie = document.cookie
          .split(";")
          .find((c) => c.trim().startsWith("pwa_context="));
        if (contextCookie) {
          const ctx = contextCookie.trim().substring("pwa_context=".length);
          return ctx.split("/")[1] || ctx;
        }
      } catch (e) {}
      const match = window.location.href.match(/\/(https|http)\/([^/]+)/);
      return match ? match[2] : window.location.hostname || "default";
    },

    async getItem(key) {
      const res = await bridge.invoke("kv_get", { appId: this._appId, key });
      return res.success ? res.data : null;
    },

    async setItem(key, value) {
      const res = await bridge.invoke("kv_set", {
        appId: this._appId,
        key,
        value: String(value),
      });
      return res.success;
    },

    async removeItem(key) {
      const res = await bridge.invoke("kv_remove", { appId: this._appId, key });
      return res.success;
    },

    async clear() {
      const res = await bridge.invoke("kv_clear", { appId: this._appId });
      return res.success;
    },
  };
}

export function hackIndexedDB() {
  const appId = (() => {
    try {
      const contextCookie = document.cookie
        .split(";")
        .find((c) => c.trim().startsWith("pwa_context="));
      if (contextCookie) {
        const ctx = contextCookie.trim().substring("pwa_context=".length);
        return ctx.replace(/\//g, "-").replace(/\./g, "_");
      }
    } catch (e) {}
    const match = window.location.href.match(/\/(https|http)\/([^/]+)/);
    return match ? `${match[1]}-${match[2].replace(/\./g, "_")}` : "default";
  })();

  const originalOpen = IDBFactory.prototype.open;
  IDBFactory.prototype.open = function (name, version) {
    const prefixedName = `${appId}_${name}`;
    return originalOpen.call(this, prefixedName, version);
  };

  const originalDelete = IDBFactory.prototype.deleteDatabase;
  IDBFactory.prototype.deleteDatabase = function (name) {
    return originalDelete.call(this, `${appId}_${name}`);
  };
}

export async function hackLocalStorage(bridge) {
  const appId = (() => {
    try {
      const contextCookie = document.cookie
        .split(";")
        .find((c) => c.trim().startsWith("pwa_context="));
      if (contextCookie) {
        const ctx = contextCookie.trim().substring("pwa_context=".length);
        return ctx.replace(/\//g, "-").replace(/\./g, "_");
      }
    } catch (e) {}
    const match = window.location.href.match(/\/(https|http)\/([^/]+)/);
    return match ? `${match[1]}-${match[2].replace(/\./g, "_")}` : "default";
  })();

  try {
    const res = await bridge.invoke("kv_get_all", { appId });
    if (res.success && res.data) {
      Object.entries(res.data).forEach(([key, value]) => {
        if (!localStorage.getItem(key)) {
          localStorage.setItem(key, value);
        }
      });
    }
  } catch (e) {
    console.error("[PWA Hack] Failed to restore storage:", e);
  }

  const originalSetItem = Storage.prototype.setItem;
  const originalRemoveItem = Storage.prototype.removeItem;
  const originalClear = Storage.prototype.clear;

  Storage.prototype.setItem = function (key, value) {
    originalSetItem.call(this, key, value);
    bridge.invoke("kv_set", { appId, key, value: String(value) }).catch(() => {});
  };

  Storage.prototype.removeItem = function (key) {
    originalRemoveItem.call(this, key);
    bridge.invoke("kv_remove", { appId, key }).catch(() => {});
  };

  Storage.prototype.clear = function () {
    originalClear.call(this);
    bridge.invoke("kv_clear", { appId }).catch(() => {});
  };
}
