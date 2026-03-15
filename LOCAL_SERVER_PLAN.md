# 本地 HTTP 服务器方案

## 问题背景

当前 Tauri 移动端存在以下问题：

1. **自定义协议阻塞**：`register_uri_scheme_protocol` 是同步单线程，一个大文件请求卡住所有其他请求
2. **发热严重**：WebView ↔ Rust ↔ 系统 多次桥接，音频文件全量读入内存
3. **不支持 Range 请求**：audio/video 无法跳着解码播放
4. **后台被杀**：没有前台服务支持

## 方案对比

| 方案 | 并发处理 | Range 支持 | 发热 | 后台播放 | 复杂度 |
|------|---------|-----------|------|---------|--------|
| **自定义协议** (current) | ❌ 单线程阻塞 | ❌ | ❌ 高 | ❌ | 低 |
| **本地 HTTP 服务器** | ✅ 多线程异步 | ✅ | ✅ 低 | ✅ | 中 |

## 本地服务器优势

### 1. 无并发阻塞
```
Tauri 自定义协议：
请求 A (大文件) ──→ 阻塞 ──→ 请求 B 等待 ──→ 请求 C 等待

本地 HTTP 服务器：
请求 A ──→ 线程 1 处理
请求 B ──→ 线程 2 处理 (并行)
请求 C ──→ 线程 3 处理 (并行)
```

### 2. 原生支持 HTTP 特性
- **Range 请求**：audio/video 可以跳着解码，支持进度拖动
- **Stream 传输**：大文件分块传输，不占用内存
- **Keep-Alive**：连接复用，减少开销

### 3. 统一架构
| 平台 | 实现 | WebView 访问 |
|------|------|-------------|
| Android | Kotlin + NanoHTTPD | `http://localhost:8080/` |
| Linux (Tauri) | Rust + warp/axum | `http://localhost:8080/` |
| 前端 | 统一代码 | 同一套 API |

## 实现思路

### 端点设计

```
http://localhost:8080/
├── /                    → 前端入口 (index.html)
├── /api/proxy?url=xxx   → 代理远程请求（带 CORS）
├── /api/audio?url=xxx   → 音频流式代理（支持 Range）
├── /api/image?url=xxx   → 图片代理（带缓存）
└── /cache/xxx           → 本地缓存文件
```

### Android (Kotlin)

```kotlin
class ProxyServer(port: Int) : NanoHTTPD(port) {
    override fun serve(session: IHTTPSession): Response {
        return when (session.uri) {
            "/api/proxy" -> {
                val url = session.parameters["url"]?.first()
                val response = URL(url).openConnection().getInputStream()
                newChunkedResponse(OK, "application/octet-stream", response)
                    .apply { addHeader("Access-Control-Allow-Origin", "*") }
            }
            // ... 其他端点
            else -> newFixedLengthResponse(NOT_FOUND, "text/plain", "404")
        }
    }
}
```

### Linux (Rust)

```rust
use warp::Filter;

let proxy = warp::path("api")
    .and(warp::path("proxy"))
    .and(warp::query::<HashMap<String, String>>())
    .and_then(|params: HashMap<String, String>| async move {
        let url = params.get("url")?;
        let response = reqwest::get(url).await.ok()?;
        Ok::<_, warp::Rejection>(response)
    });

warp::serve(proxy).run(([127,0,0,1], 8080)).await;
```

### 前端适配

```javascript
// 统一 API 封装
const API_BASE = 'http://localhost:8080';

// 代理远程图片
const getProxiedImage = (url) => 
    `${API_BASE}/api/proxy?url=${encodeURIComponent(url)}`;

// 代理音频（支持 Range，不发热）
const playAudio = (url) => {
    const audio = new Audio(`${API_BASE}/api/audio?url=${encodeURIComponent(url)}`);
    audio.play();
};
```

## 预期收益

1. **并发问题解决**：多线程异步处理，大文件下载不阻塞 UI
2. **发热降低**：流式传输，不占用内存
3. **音频/视频正常**：支持 Range 请求，可以后台播放
4. **代码统一**：Android 和 Linux 前端代码完全一致

## 下一步

1. [ ] 实现 Android Kotlin 本地服务器版本
2. [ ] 实现 Linux Rust 本地服务器版本
3. [ ] 统一前端 API 层
4. [ ] 对比测试性能和发热

## 参考

- [NanoHTTPD](https://github.com/NanoHttpd/nanohttpd) - Android 轻量 HTTP 服务器
- [warp](https://github.com/seanmonstar/warp) - Rust 异步 HTTP 框架
- HTTP Range Requests: https://developer.mozilla.org/en-US/docs/Web/HTTP/Range_requests
