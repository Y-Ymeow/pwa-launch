# PWA Container 开发计划

> 生成时间: 2026-03-13
> 状态: 待办 (不提交到 git)

---

## 1. 文件系统能力重构 (高优先级)

### 问题
- Android content:// URI 读取仍有问题
- 本地 HTTP 服务器方案复杂且有端口占用风险

### 解决方案
直接用 Tauri 的 FS 能力读取文件，不经过本地服务器

### 具体任务
- [ ] 新增 `fs://` 自定义协议
  - 格式: `fs://localhost/<file-id>` 或 `fs://localhost?path=<encoded-path>`
  - Rust 端直接读取文件内容返回
  - 支持 Range 请求（音视频需要）
  
- [ ] 新增命令
  - `fs_read_file(path): Promise<Uint8Array>` - 读取文件为二进制
  - `fs_read_file_url(path): Promise<string>` - 返回 fs:// URL
  - `fs_get_file_info(path): Promise<{size, mimeType}>` - 获取文件信息

- [ ] 修改 `resolve_local_file_url`
  - 音视频文件: 返回 `fs://` URL (支持 Range)
  - 图片文件: 直接读取返回 blob URL（或保持 static://）

### 优势
- 不需要启动本地服务器
- 不需要处理端口占用
- 手机端权限更简单

---

## 2. adapt.js 模块化重构 (中优先级)

### 问题
- adapt.js 已经超过 800 行，越来越难维护
- 所有功能混在一起，不容易按需使用

### 解决方案
拆分成多个模块，编译时拼装

### 目录结构
```
src-tauri/
  adapt/
    core.js       # 核心: bridge, invoke, postMessage
    fs.js         # 文件系统: fs_read_file, openFileDialog
    network.js    # 网络: fetch代理, XHR代理, static协议
    media.js      # 媒体: 图片代理, 音视频处理
    ui.js         # UI: 悬浮验证按钮
    webview.js    # WebView跳转能力 (新增)
    index.js      # 入口，按条件加载各模块
```

### 编译时拼装
```rust
// lib.rs
const ADAPT_CORE: &str = include_str!("adapt/core.js");
const ADAPT_FS: &str = include_str!("adapt/fs.js");
const ADAPT_NETWORK: &str = include_str!("adapt/network.js");
// ...

let mut adapt_script = String::new();
adapt_script.push_str(ADAPT_CORE);
adapt_script.push_str(ADAPT_FS);
adapt_script.push_str(ADAPT_NETWORK);
// 根据配置选择性加载
```

### 任务清单
- [ ] 创建 `src-tauri/adapt/` 目录
- [ ] 将现有 adapt.js 拆分到各模块
- [ ] 修改 lib.rs 拼装逻辑
- [ ] 添加 feature flags 控制加载哪些模块

---

## 3. WebView 跳转能力 (高优先级)

### 需求
- 从 PWA 内部打开新 WebView 窗口
- 用于漫画阅读器: 点击章节在新窗口打开阅读页
- 用于视频站: 点击视频在新窗口播放

### API 设计
```javascript
// adapt.js 新增 API
window.__TAURI__.webview = {
  // 打开新 WebView 窗口
  open: async (options) => {
    return invoke('open_webview', {
      url: options.url,
      title: options.title,
      width: options.width || 800,
      height: options.height || 600,
      // 是否启用 adapt.js 注入
      injectAdapt: options.injectAdapt !== false,
    });
  },
  
  // 关闭当前 WebView
  close: async () => {
    return invoke('close_current_webview');
  },
  
  // 在当前 WebView 加载新 URL
  loadUrl: async (url) => {
    return invoke('webview_load_url', { url });
  }
};
```

### Rust 实现
- [ ] 新增命令 `open_webview`
- [ ] Android: 使用 `WebViewActivity` 或类似机制
- [ ] Desktop: 使用 Tauri 的 Window API
- [ ] 新 WebView 也要注入 adapt.js

### 使用场景
```javascript
// 漫画阅读器示例
const chapters = await fetchChapters();
chapters.forEach(ch => {
  btn.onclick = () => {
    window.__TAURI__.webview.open({
      url: ch.readUrl,
      title: ch.title,
      width: 1200,
      height: 800,
      injectAdapt: true  // 新窗口也需要代理能力
    });
  };
});
```

---

## 4. 图片/资源转发优化 (中优先级)

### 当前问题
- 大图片通过 proxy_fetch 会阻塞
- static:// 协议在线程中执行但仍需等待完整下载

### 优化方案
使用 FS 能力的思路：
- 对于需要转发的资源，先下载到临时文件
- 返回 `fs://` URL 指向临时文件
- WebView 通过 fs 协议流式读取

### 实现
```javascript
// network.js
async function proxyImageToFs(url) {
  // 1. 通过 Rust 下载到临时目录
  const tempPath = await invoke('download_to_temp', { url });
  // 2. 返回 fs:// URL
  return `fs://localhost${tempPath}`;
}
```

---

## 5. 其他待办

### 紧急
- [ ] 修复 Android content URI 读取（如上述方案1）

### 功能增强
- [ ] 代理设置页面支持测试连接
- [ ] 添加 "清除缓存" 功能
- [ ] PWA 添加刷新按钮

### 代码质量
- [ ] 统一错误处理
- [ ] 添加更多日志
- [ ] 代码文档注释

---

## 实施顺序建议

1. **先做方案1** (FS能力) - 解决手机端文件读取问题
2. **然后方案3** (WebView跳转) - 为漫画阅读器做准备
3. **最后方案2** (adapt模块化) - 代码重构，不影响功能

---

## 备注

- 所有修改都先在桌面端测试通过再测 Android
- Android 测试重点关注权限和 content URI
- 保持向后兼容，现有 PWA 不应受影响
