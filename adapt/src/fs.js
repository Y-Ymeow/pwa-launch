/**
 * File system APIs - 使用 OPFS (Origin Private File System)
 * 无需 Tauri 命令，纯前端实现
 */

// 获取 OPFS 根目录
async function getOPFSRoot() {
  if (!navigator.storage || !navigator.storage.getDirectory) {
    throw new Error("OPFS not supported on this platform");
  }
  return await navigator.storage.getDirectory();
}

// 解析路径获取文件/目录句柄
async function resolvePath(path, create = false) {
  const parts = path.split("/").filter(p => p && p !== ".");
  const root = await getOPFSRoot();
  
  if (parts.length === 0) return { dir: root, name: "" };
  
  let currentDir = root;
  for (let i = 0; i < parts.length - 1; i++) {
    currentDir = await currentDir.getDirectoryHandle(parts[i], { create });
  }
  
  return { dir: currentDir, name: parts[parts.length - 1] };
}

export function createFS(bridge) {
  return {
    async readDir(path) {
      try {
        const { dir, name } = await resolvePath(path);
        const targetDir = name ? await dir.getDirectoryHandle(name) : dir;
        const entries = [];
        
        for await (const [entryName, handle] of targetDir.entries()) {
          entries.push({
            name: entryName,
            isDirectory: handle.kind === "directory",
            isFile: handle.kind === "file",
          });
        }
        return entries;
      } catch (e) {
        console.error("[FS] readDir error:", e);
        return [];
      }
    },

    async readFile(path, options = {}) {
      try {
        const { dir, name } = await resolvePath(path);
        const fileHandle = await dir.getFileHandle(name);
        const file = await fileHandle.getFile();
        
        if (options.encoding === "utf8") {
          return await file.text();
        }
        
        const arrayBuffer = await file.arrayBuffer();
        return new Uint8Array(arrayBuffer);
      } catch (e) {
        throw new Error(`Read failed: ${e.message}`);
      }
    },

    async readFileRange(path, offset = 0, length = 262144) {
      try {
        // 使用后端命令读取文件范围
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
      } catch (e) {
        console.error("[FS] readFileRange error:", e);
        return null;
      }
    },

    async writeFile(path, content, options = {}) {
      try {
        const { dir, name } = await resolvePath(path, true);
        const fileHandle = await dir.getFileHandle(name, { create: true });
        const writable = await fileHandle.createWritable();
        
        if (typeof content === "string") {
          await writable.write(content);
        } else if (content instanceof Uint8Array || content instanceof ArrayBuffer) {
          await writable.write(content);
        } else {
          await writable.write(String(content));
        }
        
        await writable.close();
        return true;
      } catch (e) {
        throw new Error(`Write failed: ${e.message}`);
      }
    },

    async createDir(path, options = {}) {
      try {
        await resolvePath(path, options.recursive !== false);
        return true;
      } catch (e) {
        throw new Error(`Create dir failed: ${e.message}`);
      }
    },

    async removeFile(path) {
      try {
        const { dir, name } = await resolvePath(path);
        await dir.removeEntry(name);
        return true;
      } catch (e) {
        throw new Error(`Remove failed: ${e.message}`);
      }
    },

    async removeDir(path, options = {}) {
      try {
        const { dir, name } = await resolvePath(path);
        await dir.removeEntry(name, { recursive: options.recursive });
        return true;
      } catch (e) {
        throw new Error(`Remove failed: ${e.message}`);
      }
    },

    async exists(path) {
      try {
        const { dir, name } = await resolvePath(path);
        await dir.getFileHandle(name);
        return true;
      } catch {
        try {
          const { dir, name } = await resolvePath(path);
          await dir.getDirectoryHandle(name);
          return true;
        } catch {
          return false;
        }
      }
    },

    // File dialog - 使用后端 open_file_dialog 命令
    async openFileDialog(options = {}) {
      try {
        const result = await bridge.invoke("open_file_dialog", {
          title: options.title,
          multiple: options.multiple,
          filters: options.filters,
          directory: options.directory
        });
        
        if (!result.success || !result.data || result.data.paths.length === 0) {
          return null;
        }
        
        const paths = result.data.paths;
        return options.multiple ? paths : paths[0];
      } catch (e) {
        console.log("[FS] File dialog error:", e);
        return null;
      }
    },

    async readFileContent(path) {
      try {
        // 使用后端命令读取文件
        const result = await bridge.invoke("read_file_content", { path });
        
        if (result.success && result.data) {
          const byteCharacters = atob(result.data.content);
          const bytes = new Uint8Array(byteCharacters.length);
          for (let i = 0; i < byteCharacters.length; i++) {
            bytes[i] = byteCharacters.charCodeAt(i);
          }
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
      } catch (e) {
        console.error("[FS] readFileContent error:", e);
        return null;
      }
    },

    async resolveLocalFileUrl(filePath) {
      // 如果已经是 http URL，直接返回
      if (filePath.startsWith("http://") || filePath.startsWith("https://")) {
        return filePath;
      }
      
      try {
        // 使用后端命令获取本地文件 URL
        const result = await bridge.invoke("resolve_local_file_url", { path: filePath });
        if (result.success && result.data) {
          return result.data;
        }
      } catch (e) {
        console.error("[FS] resolveLocalFileUrl error:", e);
      }
      return filePath;
    },

    async pickAndResolveLocalFile(options = {}) {
      try {
        const result = await bridge.invoke("open_file_dialog", {
          title: options.title || "Select File",
          multiple: options.multiple || false,
          filters: options.types?.map(t => ({
            name: t.description || 'Files',
            extensions: Object.values(t.accept || {}).flat().map(ext => ext.replace('.', ''))
          })),
          directory: false
        });
        
        if (!result.success || !result.data || result.data.paths.length === 0) {
          return options.multiple ? [] : null;
        }
        
        const paths = result.data.paths;
        const items = paths.map(path => ({
          path,
          url: `http://localhost:19315/local/file/${encodeURIComponent(path)}`
        }));
        
        return options.multiple ? items : items[0];
      } catch (e) {
        console.error("[FS] File selection failed:", e);
        return options.multiple ? [] : null;
      }
    },

    async getFileInfo(filePath) {
      return this.readFileContent(filePath);
    },

    /**
     * 从 PWA File 对象保存到 OPFS
     * 用于 PWA <input type="file"> 获取的文件
     */
    async saveFileFromPWA(file, options = {}) {
      const arrayBuffer = await file.arrayBuffer();
      const uint8Array = new Uint8Array(arrayBuffer);

      // 保存到 OPFS 的 uploads 目录
      const tempPath = options.path || `uploads/${Date.now()}_${file.name}`;
      await this.writeFile(tempPath, uint8Array);

      return {
        path: tempPath,
        name: file.name,
        size: file.size,
        type: file.type,
      };
    },

    /**
     * 获取 OPFS 使用配额信息
     */
    async getStorageInfo() {
      if (navigator.storage && navigator.storage.estimate) {
        return await navigator.storage.estimate();
      }
      return null;
    },

    /**
     * 请求持久化存储（防止被浏览器清理）
     */
    async requestPersistentStorage() {
      if (navigator.storage && navigator.storage.persist) {
        const isPersistent = await navigator.storage.persist();
        return { isPersistent, granted: isPersistent };
      }
      return { isPersistent: false, granted: false };
    },
  };
}

// 使用后端 open_file_dialog 命令选择文件
// OPFS 用于文件存储，但选择文件需要系统对话框
export function setupFilePicker(fs, bridge) {
  // 调用后端 open_file_dialog 命令
  async function callOpenFileDialog(options = {}) {
    const result = await bridge.invoke("open_file_dialog", {
      title: options.title,
      multiple: options.multiple,
      filters: options.filters,
      directory: options.directory
    });
    
    if (!result.success || !result.data) {
      return [];
    }
    
    return result.data.paths || [];
  }

  // 包装 showOpenFilePicker - 使用后端命令
  window.showOpenFilePicker = async function (options = {}) {
    const paths = await callOpenFileDialog({
      multiple: options.multiple,
      filters: options.types?.map(t => ({
        name: t.description || 'Files',
        extensions: Object.values(t.accept || {}).flat().map(ext => ext.replace('.', ''))
      }))
    });
    
    if (paths.length === 0) {
      throw new DOMException("User cancelled", "AbortError");
    }
    
    // 返回统一格式的对象
    return paths.map((path) => ({
      kind: "file",
      name: path.split(/[\\/]/).pop() || path,
      _path: path,
      getFile: async () => {
        // 通过后端读取文件
        const content = await bridge.invoke("read_file_content", { path });
        if (content.success && content.data) {
          const binary = atob(content.data.content);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
          }
          return new File([bytes], content.data.name, { type: content.data.mimeType });
        }
        throw new Error("Failed to read file");
      },
      getURL: async () => {
        // 使用本地服务器 URL
        const result = await bridge.invoke("resolve_local_file_url", { path });
        return result.success ? result.data : null;
      },
      getPath: () => path,
    }));
  };

  // 辅助方法：使用后端命令
  window.tauriFilePicker = {
    async open(options = {}) {
      const paths = await callOpenFileDialog(options);
      
      if (paths.length === 0) {
        return options.multiple ? [] : null;
      }
      
      const items = paths.map(path => ({
        kind: "file",
        name: path.split(/[\\/]/).pop() || path,
        _path: path,
        getPath: () => path,
        getURL: async () => {
          const result = await bridge.invoke("resolve_local_file_url", { path });
          return result.success ? result.data : null;
        },
        getFile: async () => {
          const content = await bridge.invoke("read_file_content", { path });
          if (content.success && content.data) {
            const binary = atob(content.data.content);
            const bytes = new Uint8Array(binary.length);
            for (let i = 0; i < binary.length; i++) {
              bytes[i] = binary.charCodeAt(i);
            }
            return new File([bytes], content.data.name, { type: content.data.mimeType });
          }
          return null;
        }
      }));
      
      return options.multiple ? items : items[0];
    },
  };

  // 保存到 OPFS 的辅助方法
  window.saveToOPFS = async (file, path) => {
    return await fs.saveFileFromPWA(file, { path });
  };
}
