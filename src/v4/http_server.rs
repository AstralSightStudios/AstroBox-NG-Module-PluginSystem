//! Level 4 插件的内建 HTTP 服务器。
//!
//! 监听在宿主侧（hyper），收到请求后通过 driver 的命令通道转交给插件导出的
//! `astrobox:psys-plugin-v4/http#handle`。这样做而不是让插件自己开套接字，有三个
//! 原因：插件不需要 `wasi:sockets` 权限；服务器生命周期能被宿主牢牢绑在插件实例上
//! （插件停/热重载时端口一定会释放）；以及绑定地址可以强制默认只走回环。

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use tokio::net::TcpListener;
use std::sync::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::v4::bindings::astrobox::psys_host_v4::http_server as bindings;

/// 单个请求体的上限。插件的 handler 拿到的是完整 body（WIT 里就是 `list<u8>`），
/// 不设上限的话一个大 POST 就能把内存吃穿。
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// 请求从 hyper 任务送进 driver 的信封。
pub(crate) struct HttpDispatch {
    pub(crate) server_id: u32,
    pub(crate) request: bindings::Request,
    pub(crate) reply: oneshot::Sender<Result<bindings::Response, String>>,
}

struct RunningServer {
    info: bindings::ServerInfo,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

/// 一个插件名下所有在跑的服务器。
///
/// 用同步锁而不是 `tokio::sync::Mutex`：`list-servers` 在 WIT 里是同步接口，
/// 而下面几个方法都只在锁内做 map 操作、await 一律在锁外，不会阻塞执行器。
#[derive(Default)]
pub(crate) struct HttpServerRegistry {
    servers: Mutex<HashMap<u32, RunningServer>>,
    next_id: AtomicU32,
}

impl HttpServerRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn list(&self) -> Vec<bindings::ServerInfo> {
        let guard = self
            .servers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut list: Vec<_> = guard.values().map(|server| server.info.clone()).collect();
        list.sort_by_key(|info| info.id);
        list
    }

    pub(crate) async fn stop(&self, id: u32) -> Result<(), String> {
        let server = {
            let mut guard = self
                .servers
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            guard.remove(&id)
        };
        let Some(mut server) = server else {
            return Err(format!("http server {id} not found"));
        };
        if let Some(shutdown) = server.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = server.task.await;
        Ok(())
    }

    /// 停掉该插件的全部服务器。插件停止或热重载时必须调用，否则端口会一直被占着。
    pub(crate) async fn stop_all(&self) {
        let servers = {
            let mut guard = self
                .servers
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            std::mem::take(&mut *guard)
        };
        for (_, mut server) in servers {
            if let Some(shutdown) = server.shutdown.take() {
                let _ = shutdown.send(());
            }
            let _ = server.task.await;
        }
    }

    /// 起一个服务器。`dispatch_tx` 是 driver 的命令通道，请求最终从这里进插件。
    pub(crate) async fn start(
        &self,
        plugin_name: String,
        options: bindings::ServerOptions,
        dispatch_tx: mpsc::UnboundedSender<HttpDispatch>,
    ) -> Result<bindings::ServerInfo, String> {
        let host_ip = if options.bind_all_interfaces {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let addr = SocketAddr::from((host_ip, options.port));

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|err| format!("failed to bind {addr}: {err}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|err| format!("failed to resolve the listening address: {err}"))?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let host = host_ip.to_string();
        let port = local_addr.port();
        let info = bindings::ServerInfo {
            id,
            host: host.clone(),
            port,
            url: format!(
                "http://{}:{}",
                if options.bind_all_interfaces {
                    "127.0.0.1"
                } else {
                    host.as_str()
                },
                port
            ),
        };

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let accept_plugin = plugin_name.clone();
        let task = tokio::spawn(async move {
            let builder = ConnBuilder::new(TokioExecutor::new());
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let (stream, _peer) = match accepted {
                            Ok(pair) => pair,
                            Err(err) => {
                                log::warn!(
                                    "[plugin:{accept_plugin}] http server {id} accept failed: {err}"
                                );
                                continue;
                            }
                        };
                        let dispatch_tx = dispatch_tx.clone();
                        let builder = builder.clone();
                        let plugin_name = accept_plugin.clone();
                        tokio::spawn(async move {
                            let service = service_fn(move |req: HyperRequest<Incoming>| {
                                let dispatch_tx = dispatch_tx.clone();
                                let plugin_name = plugin_name.clone();
                                async move { serve(plugin_name, id, dispatch_tx, req).await }
                            });
                            if let Err(err) = builder
                                .serve_connection(TokioIo::new(stream), service)
                                .await
                            {
                                log::debug!("[plugin] http server {id} connection ended: {err}");
                            }
                        });
                    }
                }
            }
            log::info!("[plugin:{accept_plugin}] http server {id} stopped");
        });

        let mut guard = self
            .servers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        guard.insert(
            id,
            RunningServer {
                info: info.clone(),
                shutdown: Some(shutdown_tx),
                task,
            },
        );
        drop(guard);

        log::info!(
            "[plugin:{}] http server {} listening on {}:{}",
            plugin_name,
            id,
            host,
            port
        );
        Ok(info)
    }
}

/// 把一次 hyper 请求转成 WIT 记录，交给插件，再把结果转回去。
async fn serve(
    plugin_name: String,
    server_id: u32,
    dispatch_tx: mpsc::UnboundedSender<HttpDispatch>,
    req: HyperRequest<Incoming>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let (parts, body) = req.into_parts();

    let collected = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            log::warn!("[plugin:{plugin_name}] http server {server_id} body read failed: {err}");
            return Ok(text_response(StatusCode::BAD_REQUEST, "failed to read body"));
        }
    };
    if collected.len() > MAX_BODY_BYTES {
        return Ok(text_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body too large",
        ));
    }

    let headers = parts
        .headers
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|value| bindings::Header {
                name: name.as_str().to_string(),
                value: value.to_string(),
            })
        })
        .collect();

    let request = bindings::Request {
        method: parts.method.as_str().to_ascii_uppercase(),
        path: parts.uri.path().to_string(),
        query: parts.uri.query().unwrap_or_default().to_string(),
        headers,
        body: collected.to_vec(),
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    if dispatch_tx
        .send(HttpDispatch {
            server_id,
            request,
            reply: reply_tx,
        })
        .is_err()
    {
        // driver 没了说明插件正在停，这时候连接还没关掉是正常的竞态。
        return Ok(text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "plugin is not running",
        ));
    }

    match reply_rx.await {
        Ok(Ok(response)) => {
            let status =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut builder = HyperResponse::builder().status(status);
            for header in response.headers {
                builder = builder.header(header.name, header.value);
            }
            Ok(builder
                .body(Full::new(Bytes::from(response.body)))
                .unwrap_or_else(|err| {
                    log::warn!(
                        "[plugin:{plugin_name}] http server {server_id} produced an invalid response: {err}"
                    );
                    text_response(StatusCode::INTERNAL_SERVER_ERROR, "invalid plugin response")
                }))
        }
        Ok(Err(err)) => {
            log::warn!("[plugin:{plugin_name}] http server {server_id} handler failed: {err}");
            Ok(text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "plugin handler failed",
            ))
        }
        Err(_) => Ok(text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "plugin is not running",
        )),
    }
}

fn text_response(status: StatusCode, message: &str) -> HyperResponse<Full<Bytes>> {
    HyperResponse::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(message.to_string())))
        .expect("static response is always valid")
}
