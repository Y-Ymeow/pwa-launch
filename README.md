# PWA Container

跨平台 PWA 应用容器 - 解放 PWA 的限制！

## 功能特性

- ✅ **跨平台支持**: Windows, macOS, Linux, iOS, Android
- ✅ **无跨域限制**: 原生网络请求，无 CORS 限制
- ✅ **文件系统访问**: 完整的本地文件系统访问能力
- ✅ **应用隔离**: 每个应用独立的 WebView 进程 + SQLite 数据库 + 文件存储
- ✅ **独立进程**: 每个 PWA 启动独立 WebView 窗口，互不影响，不会累积内存
- ✅ **数据管理**: 一键清除数据、备份/恢复数据
- ✅ **快捷方式**: 创建桌面快捷方式快速启动
- ✅ **轻量级**: 使用系统 WebView，内存占用远小于浏览器
- ✅ **运行监控**: 实时显示运行中的 PWA 窗口，可手动关闭

## 技术栈

- **前端**: React + TypeScript + Vite
- **后端**: Rust + Tauri v2
- **数据库**: SQLite (rusqlite)
- **WebView**: 系统原生 WebView (EdgeWebView/Safari/WebKit)

## 项目结构

```
pwa-webview-app/
├── src/
│   ├── main.rs          # Tauri 应用入口
│   ├── commands.rs      # Tauri 命令处理
│   ├── db.rs            # 数据库操作
│   ├── models.rs        # 数据模型
│   └── utils.rs         # 工具函数
├── src/frontend/        # React 前端
│   ├── main.tsx
│   ├── App.tsx
│   └── styles/
├── Cargo.toml           # Rust 依赖
├── package.json         # Node.js 依赖
├── tauri.conf.json      # Tauri 配置
└── vite.config.ts       # Vite 配置
```

## 开发指南

### 环境要求

- Rust 1.70+
- Node.js 18+
- Tauri CLI: `cargo install tauri-cli`

### 安装依赖

```bash
# 安装 Node.js 依赖
npm install

# 确保 Rust 依赖已安装 (Cargo.toml)
```

### 开发模式

```bash
# 启动开发服务器
npm run tauri dev
```

### 构建应用

```bash
# 构建所有平台
npm run tauri build

# 构建特定平台
npm run tauri build -- --target x86_64-pc-windows-msvc
npm run tauri build -- --target x86_64-apple-darwin
npm run tauri build -- --target x86_64-unknown-linux-gnu
```

## API 说明

### Tauri 命令

| 命令 | 说明 |
|------|------|
| `install_pwa` | 安装 PWA 应用 |
| `uninstall_pwa` | 卸载应用 |
| `list_apps` | 获取应用列表 |
| `launch_app` | 启动应用（创建独立 WebView 窗口） |
| `close_pwa_window` | 关闭指定的 PWA 窗口 |
| `list_running_pwas` | 获取所有运行中的 PWA 窗口 |
| `clear_data` | 清除应用数据 |
| `backup_data` | 备份应用数据 |
| `restore_data` | 恢复应用数据 |
| `create_shortcut` | 创建桌面快捷方式 |
| `get_app_info` | 获取应用详情 |
| `update_pwa` | 更新应用 |

## 数据存储

- **应用数据目录**: `{AppData}/com.pwa.container/apps/{app_id}/`
  - `data.db` - 应用专属 SQLite 数据库
  - `files/` - 应用文件存储
  - `cache/` - 应用缓存
- **备份目录**: `{AppData}/com.pwa.container/backups/`
- **主数据库**: `{AppData}/com.pwa.container/pwa_container.db`

## 跨平台构建

### Windows
```bash
npm run tauri build -- --target x86_64-pc-windows-msvc
```
输出：`.msi` 和 `.exe` 安装包

### macOS
```bash
npm run tauri build -- --target x86_64-apple-darwin
npm run tauri build -- --target aarch64-apple-darwin  # Apple Silicon
```
输出：`.app` 和 `.dmg`

### Linux
```bash
npm run tauri build -- --target x86_64-unknown-linux-gnu
```
输出：`.deb` 和 `.AppImage`

## GitHub Actions 自动构建

项目配置了 GitHub Actions 自动构建工作流，使用 **Tauri 官方 action** 构建。

### 触发方式

```bash
# 推送标签触发构建
git tag v0.1.0
git push origin v0.1.0

# 或手动触发：Actions -> Build Linux & Android -> Run workflow
```

### 构建产物

**Linux**:
- `.AppImage` 便携版（无需安装，直接运行）

**Android** (Debug 版本):
- Debug APK（不需要 keystore，使用默认 debug 签名）
- 支持架构：arm64-v8a, armeabi-v7a, x86_64, x86

### 下载构建产物

1. 打开 GitHub Actions 页面
2. 点击对应的构建工作流
3. 下载 `Artifacts` 中的文件

### 本地构建 Android（可选）

如果需要本地构建测试版本：

```bash
# 设置环境变量
export ANDROID_HOME=$HOME/Android/Sdk
export NDK_HOME=$HOME/Android/ndk

# 构建 Debug APK（不需要 keystore）
npm run tauri android build -- --debug

# 产物位置
ls target/aarch64-linux-android/debug/*.apk
```

> ⚠️ **注意**: Debug 版本使用测试签名，不能上架应用商店，仅用于测试。

## 许可证

MIT
