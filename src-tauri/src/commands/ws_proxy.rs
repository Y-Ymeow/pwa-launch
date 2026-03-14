use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::{extract_domain, CommandResponse, CookieStore, ProxyConfig};

pub type WsProxyState = Arc<RwLock<Option<WsProxyHandle>>>;

pub struct WsProxyHandle {
    pub port: u16,
    pub shutdown_tx: broadcast::Sender<()>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WsProxyMessage {
    pub id: String,
    pub r#type: String,
    pub url: Option<String>,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub status: Option<u16>,
    pub headers_out: Option<HashMap<String, String>>,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WsProxyStartResponse {
    pub port: u16,
    pub ws_url: String,
}

async fn proxy_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
    cookie_store: &CookieStore,
    proxy_config: &ProxyConfig,
) -> Result<(u16, HashMap<String, String>, Vec<u8>), String> {
    let domain = extract_domain(url);
    let cookies = cookie_store.read().await;
    let cookie_header = cookies
        .get("default")
        .and_then(|app_cookies| app_cookies.get(&domain))
        .map(|c| {
            c.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    drop(cookies);

    let mut client_builder = reqwest::Client::builder().default_headers({
        let mut headers = reqwest::header::HeaderMap::new();
        if !cookie_header.is_empty() {
            headers.insert(reqwest::header::COOKIE, cookie_header.parse().unwrap());
        }
        headers
    });

    let proxy = proxy_config.read().await;
    if let Some(proxy_settings) = proxy.as_ref() {
        if proxy_settings.enabled {
            let proxy_url = proxy_settings.get_proxy_url();
            client_builder = client_builder.proxy(
                reqwest::Proxy::all(&proxy_url).map_err(|e| format!("代理配置失败：{}", e))?,
            );
        }
    }
    drop(proxy);

    let client = client_builder
        .build()
        .map_err(|e| format!("创建客户端失败：{}", e))?;

    let mut req_builder = match method {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        "HEAD" => client.head(url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, url),
        _ => client.get(url),
    };

    let has_referer = headers.keys().any(|k| k.to_lowercase() == "referer");

    for (key, value) in headers {
        if key.to_lowercase() != "cookie" {
            if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                if let Ok(header_value) = reqwest::header::HeaderValue::from_str(&value) {
                    req_builder = req_builder.header(header_name, header_value);
                }
            }
        }
    }

    if !has_referer && !domain.is_empty() {
        req_builder = req_builder.header("Referer", format!("https://{}/", domain));
    }

    if let Some(body_str) = body {
        req_builder = req_builder.body(body_str.to_string());
    }

    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;

    let status = response.status().as_u16();
    let response_headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{}", e))?;

    Ok((status, response_headers, response_bytes.to_vec()))
}

async fn handle_ws_connection(
    stream: TcpStream,
    cookie_store: CookieStore,
    proxy_config: ProxyConfig,
    shutdown_rx: broadcast::Receiver<()>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();
    let mut shutdown_rx = shutdown_rx;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let msg: WsProxyMessage = match serde_json::from_str(&text) {
                            Ok(m) => m,
                            Err(e) => {
                                log::error!("Failed to parse WS message: {}", e);
                                continue;
                            }
                        };

                        if msg.r#type == "request" {
                            let url = msg.url.clone().unwrap_or_default();
                            let method = msg.method.clone().unwrap_or_else(|| "GET".to_string());
                            let headers = msg.headers.clone().unwrap_or_default();
                            let body = msg.body.as_deref();
                            let id = msg.id.clone();

                            let result = proxy_request(
                                &url,
                                &method,
                                &headers,
                                body,
                                &cookie_store,
                                &proxy_config,
                            ).await;

                            let response = match result {
                                Ok((status, headers_out, data)) => {
                                    let content_type = headers_out.get("content-type")
                                        .map(|s| s.to_lowercase())
                                        .unwrap_or_default();

                                    let is_binary = content_type.starts_with("video/")
                                        || content_type.starts_with("audio/")
                                        || content_type.starts_with("application/octet-stream");

                                    let data_str = if is_binary {
                                        use base64::Engine;
                                        base64::engine::general_purpose::STANDARD.encode(&data)
                                    } else {
                                        String::from_utf8_lossy(&data).to_string()
                                    };

                                    WsProxyMessage {
                                        id,
                                        r#type: "response".to_string(),
                                        url: None,
                                        method: None,
                                        headers: None,
                                        body: None,
                                        status: Some(status),
                                        headers_out: Some(headers_out),
                                        data: Some(data_str),
                                        error: None,
                                    }
                                }
                                Err(e) => WsProxyMessage {
                                    id,
                                    r#type: "error".to_string(),
                                    url: None,
                                    method: None,
                                    headers: None,
                                    body: None,
                                    status: None,
                                    headers_out: None,
                                    data: None,
                                    error: Some(e),
                                }
                            };

                            if let Ok(json) = serde_json::to_string(&response) {
                                let _ = write.send(Message::Text(json)).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        log::error!("WS error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

#[tauri::command]
pub async fn start_ws_proxy(
    cookie_store: State<'_, CookieStore>,
    proxy_config: State<'_, ProxyConfig>,
    ws_proxy_state: State<'_, WsProxyState>,
) -> Result<CommandResponse<WsProxyStartResponse>, String> {
    let mut state = ws_proxy_state.write().await;

    if let Some(handle) = state.as_ref() {
        return Ok(CommandResponse::success(WsProxyStartResponse {
            port: handle.port,
            ws_url: format!("ws://127.0.0.1:{}", handle.port),
        }));
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定端口失败：{}", e))?;

    let addr = listener
        .local_addr()
        .map_err(|e| format!("获取地址失败：{}", e))?;

    let port = addr.port();

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();
    let cookie_store = cookie_store.inner().clone();
    let proxy_config = proxy_config.inner().clone();

    *state = Some(WsProxyHandle {
        port,
        shutdown_tx: shutdown_tx_clone,
    });

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let cookie_store = cookie_store.clone();
                            let proxy_config = proxy_config.clone();
                            let shutdown_rx = shutdown_tx.subscribe();

                            tokio::spawn(handle_ws_connection(
                                stream,
                                cookie_store,
                                proxy_config,
                                shutdown_rx,
                            ));
                        }
                        Err(e) => {
                            log::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    });

    log::info!("WebSocket 代理启动在端口 {}", port);

    Ok(CommandResponse::success(WsProxyStartResponse {
        port,
        ws_url: format!("ws://127.0.0.1:{}", port),
    }))
}

#[tauri::command]
pub async fn stop_ws_proxy(
    ws_proxy_state: State<'_, WsProxyState>,
) -> Result<CommandResponse<bool>, String> {
    let mut state = ws_proxy_state.write().await;

    if let Some(handle) = state.take() {
        let _ = handle.shutdown_tx.send(());
        log::info!("WebSocket 代理已停止");
    }

    Ok(CommandResponse::success(true))
}
