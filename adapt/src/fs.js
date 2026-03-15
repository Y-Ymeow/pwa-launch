/**
 * File system APIs
 */

export function createFS(bridge) {
  return {
    async readDir(path) {
      const res = await bridge.invoke("fs_read_dir", { path });
      return res.success ? res.data : [];
    },

    async readFile(path, options = {}) {
      const res = await bridge.invoke("read_file_content", { path });
      if (!res.success || !res.data) throw new Error(res.error || "Read failed");

      if (options.encoding === "utf8") {
        return atob(res.data.content);
      }

      const binary = atob(res.data.content);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      return bytes;
    },

    async readFileRange(path, offset = 0, length = 262144) {
      const result = await bridge.invoke("read_file_range", { path, offset, length });

      if (result.success && result.data) {
        const byteCharacters = atob(result.data.content);
        const bytes = new Uint8Array(byteCharacters.length);
        for (let i = 0; i < byteCharacters.length; i++) {
          bytes[i] = byteCharacters.charCodeAt(i);
        }
        return {
          name: result.data.name,
          path: result.data.path,
          size: result.data.size,
          offset: result.data.offset,
          length: result.data.length,
          bytes,
          arrayBuffer: bytes.buffer,
        };
      }
      return null;
    },

    async writeFile(path, content, options = {}) {
      let payload = content;
      let isBinary = true;

      if (typeof content === "string") {
        if (options.encoding === "utf8") {
          isBinary = false;
        } else {
          payload = content;
        }
      } else if (content instanceof Uint8Array || content instanceof ArrayBuffer) {
        const bytes = content instanceof ArrayBuffer ? new Uint8Array(content) : content;
        let binary = "";
        for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i]);
        payload = btoa(binary);
      }

      const res = await bridge.invoke("fs_write_file", { path, content: payload, isBinary });
      if (!res.success) throw new Error(res.error || "Write failed");
      return true;
    },

    async createDir(path, options = {}) {
      const res = await bridge.invoke("fs_create_dir", { path, recursive: options.recursive || false });
      if (!res.success) throw new Error(res.error || "Create dir failed");
      return true;
    },

    async removeFile(path) {
      const res = await bridge.invoke("fs_remove", { path, recursive: false });
      if (!res.success) throw new Error(res.error || "Remove failed");
      return true;
    },

    async removeDir(path, options = {}) {
      const res = await bridge.invoke("fs_remove", { path, recursive: options.recursive || false });
      if (!res.success) throw new Error(res.error || "Remove failed");
      return true;
    },

    async exists(path) {
      const res = await bridge.invoke("fs_exists", { path });
      return res.success ? res.data : false;
    },

    // File dialog
    async openFileDialog(options = {}) {
      const result = await bridge.invoke("open_file_dialog", {
        title: options.title,
        multiple: options.multiple,
        filters: options.filters,
        directory: options.directory,
      });

      if (result.success && result.data && result.data.paths) {
        return options.multiple ? result.data.paths : result.data.paths[0];
      }
      return null;
    },

    async readFileContent(path) {
      const result = await bridge.invoke("read_file_content", { path });

      if (result.success && result.data) {
        const byteCharacters = atob(result.data.content);
        const bytes = new Uint8Array(byteCharacters.length);
        for (let i = 0; i < byteCharacters.length; i++) bytes[i] = byteCharacters.charCodeAt(i);
        const blob = new Blob([bytes], { type: result.data.mimeType });
        return {
          name: result.data.name,
          path: result.data.path,
          size: result.data.size,
          mimeType: result.data.mimeType,
          blob,
        };
      }
      return null;
    },

    async resolveLocalFileUrl(filePath) {
      // 如果已经是 http://localhost URL，直接返回
      if (filePath.startsWith("http://localhost:")) {
        return filePath;
      }

      const result = await bridge.invoke("resolve_local_file_url", { path: filePath });
      if (result.success && result.data) return result.data;
      throw new Error("Failed to resolve file URL");
    },

    async pickAndResolveLocalFile(options = {}) {
      const result = await bridge.invoke("open_file_dialog", {
        title: options.title || "Select File",
        multiple: options.multiple || false,
        filters: options.types?.map((t) => ({
          name: t.description || "Files",
          extensions: Object.values(t.accept || {}).flat(),
        })) || [],
        directory: false,
      });

      if (!result.success || !result.data || result.data.paths.length === 0) {
        throw new Error("No file selected");
      }

      const paths = result.data.paths;

      if (options.multiple) {
        const items = await Promise.all(
          paths.map(async (p) => ({ path: p, url: await this.resolveLocalFileUrl(p) })),
        );
        return items.filter((i) => i.url);
      } else {
        return { path: paths[0], url: await this.resolveLocalFileUrl(paths[0]) };
      }
    },

    async getFileInfo(filePath) {
      return this.readFileContent(filePath);
    },

    /**
     * 从 PWA File 对象保存到真实路径
     * 用于 PWA <input type="file"> 获取的文件
     */
    async saveFileFromPWA(file, options = {}) {
      const arrayBuffer = await file.arrayBuffer();
      const uint8Array = new Uint8Array(arrayBuffer);

      // 保存到临时目录
      const tempPath = options.path || `/tmp/pwa-upload/${file.name}`;
      await this.writeFile(tempPath, uint8Array);

      return {
        path: tempPath,
        name: file.name,
        size: file.size,
        type: file.type,
      };
    },
  };
}

// Polyfill showOpenFilePicker
export function setupFilePicker(tauriBridge) {
  window.showOpenFilePicker = async function (options = {}) {
    if (!tauriBridge._ready) {
      let attempts = 0;
      while (!tauriBridge._ready && attempts < 50) {
        await new Promise((resolve) => setTimeout(resolve, 100));
        attempts++;
      }
      if (!tauriBridge._ready) {
        throw new DOMException("Tauri bridge not ready", "NotAllowedError");
      }
    }

    // 在 Android 上直接使用 Tauri 的文件选择器
    const isAndroid = navigator.userAgent.toLowerCase().includes('android');
    
    try {
      const result = await tauriBridge.pickAndResolveLocalFile(options);

      if (!result || (Array.isArray(result) && result.length === 0)) {
        throw new DOMException("No file selected", "AbortError");
      }

      const itemList = Array.isArray(result) ? result : [result];

      return itemList.map((item) => {
        const filePath = item.path;
        const fileUrl = item.url;
        const fileName = filePath.split(/[\\/]/).pop() || "file";

        return {
          kind: "file",
          name: fileName,
          _path: filePath,
          _url: fileUrl,
          getFile: async () => {
            const info = await tauriBridge.getFileInfo(filePath);
            if (!info) throw new Error(`Failed to read file: ${fileName}`);
            const file = new File([info.blob], info.name, { type: info.mimeType });
            file._path = filePath;
            return file;
          },
          getURL: () => fileUrl,
          getPath: () => filePath,
        };
      });
    } catch (error) {
      console.error("[PWA Adapt] File picker error:", error);
      throw new DOMException("File selection failed: " + error.message, "NotAllowedError");
    }
  };
  
  // 添加直接打开文件选择器的辅助方法
  window.tauriFilePicker = {
    async open(options = {}) {
      try {
        const result = await tauriBridge.openFileDialog(options);
        if (result && result.paths && result.paths.length > 0) {
          return result.paths.map(path => ({
            kind: "file",
            name: path.split(/[\\/]/).pop() || "file",
            _path: path,
            getPath: () => path,
            getURL: () => `file://${path}`,
          }));
        }
        return [];
      } catch (e) {
        console.error("[PWA Adapt] tauriFilePicker.open error:", e);
        return [];
      }
    }
  };
}
