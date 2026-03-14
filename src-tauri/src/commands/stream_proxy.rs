use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

use super::CommandResponse;

pub type StreamProxyState = Arc<RwLock<HashMap<String, StreamProxyHandle>>>;

pub struct StreamProxyHandle {
    pub local_port: u16,
    pub target_url: String,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StreamProxyStartRequest {
    pub target_url: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StreamProxyStartResponse {
    pub local_port: u16,
    pub proxy_url: String,
}

async fn proxy_websocket_stream(
    target_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    client_stream: tokio_tungstenite::WebSocketStream<TcpStream>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let (mut target_write, mut target_read) = target_stream.split();
    let (mut client_write, mut client_read) = client_stream.split();

    loop {
        tokio::select! {
            msg = client_read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let _ = target_write.send(Message::Binary(data)).await;
                    }
                    Some(Ok(Message::Text(text))) => {
                        let _ = target_write.send(Message::Text(text)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = target_write.close().await;
                        break;
                    }
                    Some(Err(e)) => {
                        log::error!("Client stream error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            msg = target_read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let _ = client_write.send(Message::Binary(data)).await;
                    }
                    Some(Ok(Message::Text(text))) => {
                        let _ = client_write.send(Message::Text(text)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = client_write.close().await;
                        break;
                    }
                    Some(Err(e)) => {
                        log::error!("Target stream error: {}", e);
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

async fn handle_http_proxy(
    mut client_stream: TcpStream,
    target_url: String,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 8192];

    let n = match client_stream.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    let target = match url::Url::parse(&target_url) {
        Ok(u) => u,
        Err(e) => {
            log::error!("Failed to parse target URL: {}", e);
            return;
        }
    };

    let host = target.host_str().unwrap_or("127.0.0.1");
    let port = target.port().unwrap_or(80);

    let mut target_stream = match TcpStream::connect(format!("{}:{}", host, port)).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to connect to target: {}", e);
            return;
        }
    };

    if let Err(e) = target_stream.write_all(request.as_bytes()).await {
        log::error!("Failed to forward request: {}", e);
        return;
    }

    loop {
        tokio::select! {
            result = target_stream.read(&mut buf) => {
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = client_stream.write_all(&buf[..n]).await {
                            log::error!("Failed to send response: {}", e);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

#[tauri::command]
pub async fn start_stream_proxy(
    request: StreamProxyStartRequest,
    stream_proxy_state: State<'_, StreamProxyState>,
) -> Result<CommandResponse<StreamProxyStartResponse>, String> {
    let target_url = request.target_url.clone();

    if target_url.starts_with("ws://") || target_url.starts_with("wss://") {
        let mut state = stream_proxy_state.write().await;

        for (_id, handle) in state.iter() {
            if handle.target_url == target_url {
                return Ok(CommandResponse::success(StreamProxyStartResponse {
                    local_port: handle.local_port,
                    proxy_url: format!("ws://127.0.0.1:{}", handle.local_port),
                }));
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("绑定端口失败：{}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("获取地址失败：{}", e))?;

        let port = addr.port();

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        let target_clone = target_url.clone();

        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_tx_clone.subscribe();

            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((client_stream, _)) => {
                                let target = target_clone.clone();
                                let shutdown_rx_inner = shutdown_tx_clone.subscribe();

                                tokio::spawn(async move {
                                    let client_ws = match accept_async(client_stream).await {
                                        Ok(w) => w,
                                        Err(e) => {
                                            log::error!("WebSocket handshake failed: {}", e);
                                            return;
                                        }
                                    };

                                    let (target_ws, _) = match connect_async(&target).await {
                                        Ok(w) => w,
                                        Err(e) => {
                                            log::error!("Failed to connect to target: {}", e);
                                            return;
                                        }
                                    };

                                    proxy_websocket_stream(target_ws, client_ws, shutdown_rx_inner).await;
                                });
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

        let id = uuid::Uuid::new_v4().to_string();
        state.insert(
            id,
            StreamProxyHandle {
                local_port: port,
                target_url: target_url.clone(),
                shutdown_tx,
            },
        );

        log::info!(
            "WebSocket 代理启动：{} -> ws://127.0.0.1:{}",
            target_url,
            port
        );

        return Ok(CommandResponse::success(StreamProxyStartResponse {
            local_port: port,
            proxy_url: format!("ws://127.0.0.1:{}", port),
        }));
    }

    let mut state = stream_proxy_state.write().await;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定端口失败：{}", e))?;

    let addr = listener
        .local_addr()
        .map_err(|e| format!("获取地址失败：{}", e))?;

    let port = addr.port();
    let target = target_url.clone();

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let shutdown_tx_clone = shutdown_tx.clone();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_tx_clone.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((client_stream, _)) => {
                            let target_owned = target.clone();
                            let shutdown_rx = shutdown_tx_clone.subscribe();

                            tokio::spawn(handle_http_proxy(client_stream, target_owned, shutdown_rx));
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

    let id = uuid::Uuid::new_v4().to_string();
    state.insert(
        id,
        StreamProxyHandle {
            local_port: port,
            target_url: target_url.clone(),
            shutdown_tx,
        },
    );

    log::info!("HTTP 代理启动：{} -> http://127.0.0.1:{}", target_url, port);

    Ok(CommandResponse::success(StreamProxyStartResponse {
        local_port: port,
        proxy_url: format!("http://127.0.0.1:{}", port),
    }))
}

#[tauri::command]
pub async fn stop_stream_proxy(
    id: String,
    stream_proxy_state: State<'_, StreamProxyState>,
) -> Result<CommandResponse<bool>, String> {
    let mut state = stream_proxy_state.write().await;

    if let Some(handle) = state.remove(&id) {
        let _ = handle.shutdown_tx.send(());
        log::info!("流代理已停止: {}", handle.target_url);
    }

    Ok(CommandResponse::success(true))
}

#[tauri::command]
pub async fn list_stream_proxies(
    stream_proxy_state: State<'_, StreamProxyState>,
) -> Result<CommandResponse<Vec<serde_json::Value>>, String> {
    let state = stream_proxy_state.read().await;

    let proxies: Vec<serde_json::Value> = state
        .iter()
        .map(|(id, handle)| {
            serde_json::json!({
                "id": id,
                "target_url": handle.target_url,
                "local_port": handle.local_port,
                "proxy_url": format!("http://127.0.0.1:{}", handle.local_port),
            })
        })
        .collect();

    Ok(CommandResponse::success(proxies))
}
